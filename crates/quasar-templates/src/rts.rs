//! RTS (Real-Time Strategy) game template
//!
//! Provides components and systems for building RTS games including:
//! - Unit management (health, movement, attack, abilities)
//! - Building system (production, resource generation, tech tree)
//! - Resource gathering and management
//! - Unit selection and group control
//! - Fog of war
//! - Minimap system

use glam::Vec2;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// ============================================================================
// Unit Components
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unit {
    pub unit_type: UnitType,
    pub movement_speed: f32,
    pub attack_damage: f32,
    pub attack_range: f32,
    pub attack_speed: f32,
    pub vision_range: f32,
    pub is_selected: bool,
    pub group_id: Option<u32>,
    pub patrol_points: Vec<[f32; 2]>,
    pub patrol_index: usize,
    pub attack_target: Option<u64>,
    pub move_target: Option<[f32; 2]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnitType {
    Worker,
    Infantry,
    Ranged,
    Cavalry,
    Siege,
    Hero,
    Scout,
    Transport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitAbilities {
    pub abilities: Vec<Ability>,
    pub cooldowns: HashMap<String, f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ability {
    pub name: String,
    pub cooldown: f32,
    pub range: f32,
    pub energy_cost: f32,
    pub ability_type: AbilityType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AbilityType {
    Damage {
        amount: f32,
        radius: f32,
    },
    Heal {
        amount: f32,
        radius: f32,
    },
    Buff {
        stat: BuffStat,
        multiplier: f32,
        duration: f32,
    },
    Summon {
        unit_type: UnitType,
        count: u32,
    },
    Teleport {
        max_distance: f32,
    },
    Stealth {
        duration: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuffStat {
    AttackSpeed,
    MovementSpeed,
    Damage,
    Defense,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Buffs {
    pub active_buffs: Vec<ActiveBuff>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveBuff {
    pub stat: BuffStat,
    pub multiplier: f32,
    pub remaining_duration: f32,
}

// ============================================================================
// Building Components
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Building {
    pub building_type: BuildingType,
    pub build_progress: f32,
    pub is_constructing: bool,
    pub rally_point: Option<[f32; 2]>,
    pub production_queue: Vec<ProductionItem>,
    pub production_progress: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuildingType {
    TownCenter,
    Barracks,
    ArcheryRange,
    Stable,
    SiegeWorkshop,
    Farm,
    LumberMill,
    MiningCamp,
    Blacksmith,
    Market,
    WatchTower,
    Castle,
    House,
    Wall,
    Gate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductionItem {
    pub item_type: ProductionType,
    pub time_required: f32,
    pub resource_cost: ResourceCost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProductionType {
    Unit(UnitType),
    Research(ResearchType),
    Upgrade(UpgradeType),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResearchType {
    InfantryArmor,
    InfantryAttack,
    CavalryArmor,
    CavalryAttack,
    RangedArmor,
    RangedAttack,
    SiegeAttack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UpgradeType {
    Age1,
    Age2,
    Age3,
    Age4,
    Loom,
    Wheelbarrow,
    Bloodlines,
    Husbandry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceGenerator {
    pub resource_type: ResourceType,
    pub generation_rate: f32,
    pub gather_rate: f32,
    pub max_workers: u32,
    pub current_workers: u32,
    pub resource_amount: f32,
}

impl ResourceGenerator {
    pub fn new(resource_type: ResourceType, amount: f32) -> Self {
        Self {
            resource_type,
            generation_rate: match resource_type {
                ResourceType::Food => 0.22,
                ResourceType::Wood => 0.0,
                ResourceType::Gold => 0.0,
                ResourceType::Stone => 0.0,
            },
            gather_rate: match resource_type {
                ResourceType::Food => 0.22,
                ResourceType::Wood => 0.39,
                ResourceType::Gold => 0.38,
                ResourceType::Stone => 0.36,
            },
            max_workers: match resource_type {
                ResourceType::Food => 8,
                _ => 4,
            },
            current_workers: 0,
            resource_amount: amount,
        }
    }
}

// ============================================================================
// Resource System
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceType {
    Food,
    Wood,
    Gold,
    Stone,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceCost {
    pub food: u32,
    pub wood: u32,
    pub gold: u32,
    pub stone: u32,
}

impl ResourceCost {
    pub fn new(food: u32, wood: u32, gold: u32, stone: u32) -> Self {
        Self {
            food,
            wood,
            gold,
            stone,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerResources {
    pub food: f32,
    pub wood: f32,
    pub gold: f32,
    pub stone: f32,
    pub population: u32,
    pub max_population: u32,
}

impl Default for PlayerResources {
    fn default() -> Self {
        Self {
            food: 200.0,
            wood: 200.0,
            gold: 100.0,
            stone: 200.0,
            population: 0,
            max_population: 5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceCarrying {
    pub resource_type: ResourceType,
    pub amount: f32,
    pub capacity: f32,
}

impl Default for ResourceCarrying {
    fn default() -> Self {
        Self {
            resource_type: ResourceType::Food,
            amount: 0.0,
            capacity: 10.0,
        }
    }
}

impl ResourceCarrying {
    pub fn is_full(&self) -> bool {
        self.amount >= self.capacity
    }
}

// ============================================================================
// Selection System
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct SelectionManager {
    pub selected_entities: HashSet<u64>,
    pub selection_box_start: Option<Vec2>,
    pub selection_box_end: Option<Vec2>,
    pub control_groups: HashMap<u32, HashSet<u64>>,
}

impl SelectionManager {
    pub fn select_entity(&mut self, entity: u64) {
        self.selected_entities.clear();
        self.selected_entities.insert(entity);
    }

    pub fn add_to_selection(&mut self, entity: u64) {
        self.selected_entities.insert(entity);
    }

    pub fn remove_from_selection(&mut self, entity: u64) {
        self.selected_entities.remove(&entity);
    }

    pub fn clear_selection(&mut self) {
        self.selected_entities.clear();
    }

    pub fn create_control_group(&mut self, group_id: u32) {
        self.control_groups
            .insert(group_id, self.selected_entities.clone());
    }

    pub fn select_control_group(&mut self, group_id: u32) -> bool {
        if let Some(group) = self.control_groups.get(&group_id) {
            self.selected_entities = group.clone();
            true
        } else {
            false
        }
    }

    pub fn add_to_control_group(&mut self, group_id: u32) {
        self.control_groups
            .entry(group_id)
            .or_default()
            .extend(self.selected_entities.iter().copied());
    }

    pub fn get_selection_box(&self) -> Option<(Vec2, Vec2)> {
        if let (Some(start), Some(end)) = (self.selection_box_start, self.selection_box_end) {
            Some((start, end))
        } else {
            None
        }
    }
}

// ============================================================================
// Fog of War
// ============================================================================

#[derive(Debug, Clone)]
pub struct FogOfWar {
    pub grid: Vec<Vec<FogState>>,
    pub grid_size: [u32; 2],
    pub cell_size: f32,
    pub world_size: Vec2,
}

impl FogOfWar {
    pub fn new(world_size: Vec2, cell_size: f32) -> Self {
        let grid_x = (world_size.x / cell_size).ceil() as u32;
        let grid_y = (world_size.y / cell_size).ceil() as u32;
        Self {
            grid: vec![vec![FogState::Hidden; grid_y as usize]; grid_x as usize],
            grid_size: [grid_x, grid_y],
            cell_size,
            world_size,
        }
    }

    pub fn world_to_grid(&self, world_pos: Vec2) -> Option<[u32; 2]> {
        let grid_x = (world_pos.x / self.cell_size) as i32;
        let grid_y = (world_pos.y / self.cell_size) as i32;
        if grid_x >= 0 && grid_y >= 0 {
            Some([grid_x as u32, grid_y as u32])
        } else {
            None
        }
    }

    pub fn grid_to_world(&self, grid_pos: [u32; 2]) -> Vec2 {
        Vec2::new(
            grid_pos[0] as f32 * self.cell_size + self.cell_size / 2.0,
            grid_pos[1] as f32 * self.cell_size + self.cell_size / 2.0,
        )
    }

    pub fn reveal_area(&mut self, center: Vec2, radius: f32) {
        if let Some(grid_center) = self.world_to_grid(center) {
            let cell_radius = (radius / self.cell_size).ceil() as u32;
            for x in grid_center[0].saturating_sub(cell_radius)..=grid_center[0] + cell_radius {
                for y in grid_center[1].saturating_sub(cell_radius)..=grid_center[1] + cell_radius {
                    if x < self.grid_size[0] && y < self.grid_size[1] {
                        let world_pos = self.grid_to_world([x, y]);
                        if world_pos.distance(center) <= radius {
                            if let Some(row) = self.grid.get_mut(x as usize) {
                                if let Some(cell) = row.get_mut(y as usize) {
                                    *cell = FogState::Visible;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn update_visibility(&mut self) {
        for row in &mut self.grid {
            for cell in row {
                if *cell == FogState::Visible {
                    *cell = FogState::Explored;
                }
            }
        }
    }

    pub fn is_visible(&self, world_pos: Vec2) -> bool {
        if let Some(grid_pos) = self.world_to_grid(world_pos) {
            return self
                .grid
                .get(grid_pos[0] as usize)
                .and_then(|row| row.get(grid_pos[1] as usize))
                .map(|&cell| cell == FogState::Visible)
                .unwrap_or(false);
        }
        false
    }

    pub fn is_explored(&self, world_pos: Vec2) -> bool {
        if let Some(grid_pos) = self.world_to_grid(world_pos) {
            return self
                .grid
                .get(grid_pos[0] as usize)
                .and_then(|row| row.get(grid_pos[1] as usize))
                .map(|&cell| cell != FogState::Hidden)
                .unwrap_or(false);
        }
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FogState {
    Hidden,
    Explored,
    Visible,
}

// ============================================================================
// Minimap System
// ============================================================================

#[derive(Debug, Clone)]
pub struct MinimapConfig {
    pub size: Vec2,
    pub world_bounds: (Vec2, Vec2),
    pub show_fog: bool,
    pub show_resources: bool,
    pub show_buildings: bool,
    pub show_units: bool,
}

impl MinimapConfig {
    pub fn world_to_minimap(&self, world_pos: Vec2) -> Vec2 {
        let (min_bounds, max_bounds) = self.world_bounds;
        let world_size = max_bounds - min_bounds;
        Vec2::new(
            (world_pos.x - min_bounds.x) / world_size.x * self.size.x,
            (world_pos.y - min_bounds.y) / world_size.y * self.size.y,
        )
    }

    pub fn minimap_to_world(&self, minimap_pos: Vec2) -> Vec2 {
        let (min_bounds, max_bounds) = self.world_bounds;
        let world_size = max_bounds - min_bounds;
        Vec2::new(
            minimap_pos.x / self.size.x * world_size.x + min_bounds.x,
            minimap_pos.y / self.size.y * world_size.y + min_bounds.y,
        )
    }
}

impl Default for MinimapConfig {
    fn default() -> Self {
        Self {
            size: Vec2::new(200.0, 200.0),
            world_bounds: (Vec2::new(-500.0, -500.0), Vec2::new(500.0, 500.0)),
            show_fog: true,
            show_resources: true,
            show_buildings: true,
            show_units: true,
        }
    }
}

// ============================================================================
// AI Components
// ============================================================================

#[derive(Debug, Clone)]
pub struct AiController {
    pub difficulty: AiDifficulty,
    pub strategy: AiStrategy,
    pub scout_timer: f32,
    pub attack_timer: f32,
    pub expansion_timer: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiDifficulty {
    Easy,
    Medium,
    Hard,
    Expert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiStrategy {
    Aggressive,
    Defensive,
    Economic,
    Boom,
    Rush,
}

// ============================================================================
// Combat Components
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackCooldown {
    pub timer: f32,
    pub base_cooldown: f32,
}

impl AttackCooldown {
    pub fn new(cooldown: f32) -> Self {
        Self {
            timer: 0.0,
            base_cooldown: cooldown,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.timer <= 0.0
    }

    pub fn reset(&mut self) {
        self.timer = self.base_cooldown;
    }

    pub fn tick(&mut self, delta: f32) {
        self.timer = (self.timer - delta).max(0.0);
    }
}

#[derive(Debug, Clone)]
pub struct CombatStats {
    pub attack_bonus: f32,
    pub defense_bonus: f32,
    pub armor_pierce: f32,
}

impl Default for CombatStats {
    fn default() -> Self {
        Self {
            attack_bonus: 0.0,
            defense_bonus: 0.0,
            armor_pierce: 0.0,
        }
    }
}

// ============================================================================
// Formation System
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormationType {
    Line,
    Column,
    Box,
    Wedge,
    Circle,
}

#[derive(Debug, Clone)]
pub struct Formation {
    pub formation_type: FormationType,
    pub spacing: f32,
    pub rotation: f32,
}

impl Formation {
    pub fn get_positions(&self, unit_count: usize, center: Vec2) -> Vec<Vec2> {
        let mut positions = Vec::with_capacity(unit_count);
        match self.formation_type {
            FormationType::Line => {
                let width = (unit_count.saturating_sub(1)) as f32 * self.spacing;
                let start_x = center.x - width / 2.0;
                for i in 0..unit_count {
                    positions.push(Vec2::new(start_x + i as f32 * self.spacing, center.y));
                }
            }
            FormationType::Column => {
                let height = (unit_count.saturating_sub(1)) as f32 * self.spacing;
                let start_y = center.y - height / 2.0;
                for i in 0..unit_count {
                    positions.push(Vec2::new(center.x, start_y + i as f32 * self.spacing));
                }
            }
            FormationType::Box => {
                let side = (unit_count as f32).sqrt().ceil() as usize;
                let offset = (side - 1) as f32 * self.spacing / 2.0;
                for i in 0..unit_count {
                    let row = i / side;
                    let col = i % side;
                    positions.push(Vec2::new(
                        center.x + col as f32 * self.spacing - offset,
                        center.y + row as f32 * self.spacing - offset,
                    ));
                }
            }
            FormationType::Wedge => {
                let mut row = 0;
                let mut index = 0;
                while index < unit_count {
                    let units_in_row = row + 1;
                    let start_x = center.x - row as f32 * self.spacing / 2.0;
                    for i in 0..units_in_row {
                        if index < unit_count {
                            positions.push(Vec2::new(
                                start_x + i as f32 * self.spacing,
                                center.y - row as f32 * self.spacing,
                            ));
                            index += 1;
                        }
                    }
                    row += 1;
                }
            }
            FormationType::Circle => {
                let radius = unit_count as f32 * self.spacing / (2.0 * std::f32::consts::PI);
                for i in 0..unit_count {
                    let angle = i as f32 * 2.0 * std::f32::consts::PI / unit_count as f32;
                    positions.push(Vec2::new(
                        center.x + radius * angle.cos(),
                        center.y + radius * angle.sin(),
                    ));
                }
            }
        }
        positions
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection_manager() {
        let mut manager = SelectionManager::default();

        assert!(manager.selected_entities.is_empty());

        manager.select_entity(0);
        assert!(manager.selected_entities.contains(&0));

        manager.create_control_group(1);
        manager.clear_selection();
        assert!(manager.selected_entities.is_empty());

        assert!(manager.select_control_group(1));
        assert!(manager.selected_entities.contains(&0));
    }

    #[test]
    fn test_fog_of_war() {
        let mut fog = FogOfWar::new(Vec2::new(100.0, 100.0), 10.0);

        assert!(!fog.is_visible(Vec2::new(50.0, 50.0)));

        fog.reveal_area(Vec2::new(50.0, 50.0), 20.0);
        assert!(fog.is_visible(Vec2::new(50.0, 50.0)));

        fog.update_visibility();
        assert!(!fog.is_visible(Vec2::new(50.0, 50.0)));
        assert!(fog.is_explored(Vec2::new(50.0, 50.0)));
    }

    #[test]
    fn test_minimap_config() {
        let config = MinimapConfig::default();

        let world_pos = Vec2::new(0.0, 0.0);
        let minimap_pos = config.world_to_minimap(world_pos);
        assert!((minimap_pos.x - 100.0).abs() < 0.1);
        assert!((minimap_pos.y - 100.0).abs() < 0.1);

        let converted_back = config.minimap_to_world(minimap_pos);
        assert!((converted_back.x - world_pos.x).abs() < 0.1);
        assert!((converted_back.y - world_pos.y).abs() < 0.1);
    }

    #[test]
    fn test_resource_cost() {
        let cost = ResourceCost::new(50, 30, 0, 0);
        assert_eq!(cost.food, 50);
        assert_eq!(cost.wood, 30);
        assert_eq!(cost.gold, 0);
        assert_eq!(cost.stone, 0);
    }

    #[test]
    fn test_formation_line() {
        let formation = Formation {
            formation_type: FormationType::Line,
            spacing: 2.0,
            rotation: 0.0,
        };

        let positions = formation.get_positions(5, Vec2::new(0.0, 0.0));
        assert_eq!(positions.len(), 5);
        assert!((positions[0].x - (-4.0)).abs() < 0.1);
        assert!((positions[4].x - 4.0).abs() < 0.1);
    }

    #[test]
    fn test_formation_box() {
        let formation = Formation {
            formation_type: FormationType::Box,
            spacing: 2.0,
            rotation: 0.0,
        };

        let positions = formation.get_positions(9, Vec2::new(0.0, 0.0));
        assert_eq!(positions.len(), 9);
    }

    #[test]
    fn test_resource_generator() {
        let mut gen = ResourceGenerator::new(ResourceType::Food, 1000.0);
        assert_eq!(gen.resource_type, ResourceType::Food);
        assert_eq!(gen.current_workers, 0);

        gen.current_workers = 4;
        assert!(gen.current_workers <= gen.max_workers);
    }

    #[test]
    fn test_attack_cooldown() {
        let mut cooldown = AttackCooldown::new(1.5);
        assert!(cooldown.is_ready());

        cooldown.reset();
        assert!(!cooldown.is_ready());

        cooldown.tick(1.0);
        assert!((cooldown.timer - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_unit_abilities() {
        let ability = Ability {
            name: "Fireball".to_string(),
            cooldown: 5.0,
            range: 10.0,
            energy_cost: 25.0,
            ability_type: AbilityType::Damage {
                amount: 100.0,
                radius: 3.0,
            },
        };

        assert_eq!(ability.name, "Fireball");
        if let AbilityType::Damage { amount, radius } = ability.ability_type {
            assert!((amount - 100.0).abs() < 0.01);
            assert!((radius - 3.0).abs() < 0.01);
        }
    }
}
