//! Procedural Generation Templates for Quasar Engine.
//!
//! Provides:
//! - **Procedural dungeon generator** - Room-based dungeon layouts
//! - **Procedural terrain generator** - Heightmap-based terrain
//! - **Procedural city generator** - Building placement and roads
//! - **Procedural forest generator** - Tree and foliage distribution
//! - **Procedural quest generator** - Dynamic quest creation

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcGenType {
    Dungeon,
    Terrain,
    City,
    Forest,
    Village,
    Cave,
    SpaceStation,
    Quest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcGenConfig {
    pub name: String,
    pub gen_type: ProcGenType,
    pub seed: u64,
    pub size: [f32; 3],
    pub density: f32,
    pub complexity: f32,
    pub custom_params: HashMap<String, f32>,
}

impl Default for ProcGenConfig {
    fn default() -> Self {
        Self {
            name: "Procedural".to_string(),
            gen_type: ProcGenType::Dungeon,
            seed: 0,
            size: [100.0, 10.0, 100.0],
            density: 0.5,
            complexity: 0.5,
            custom_params: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedEntity {
    pub entity_type: String,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub properties: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedRoom {
    pub id: u32,
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub room_type: String,
    pub connections: Vec<u32>,
    pub entities: Vec<GeneratedEntity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcGenResult {
    pub config: ProcGenConfig,
    pub rooms: Vec<GeneratedRoom>,
    pub entities: Vec<GeneratedEntity>,
    pub paths: Vec<Vec<[f32; 3]>>,
    pub spawn_points: Vec<[f32; 3]>,
}

pub struct ProcGenTemplate {
    pub name: String,
    pub gen_type: ProcGenType,
    pub description: String,
    pub default_config: ProcGenConfig,
    pub preview_image: Option<String>,
}

impl ProcGenTemplate {
    pub fn dungeon() -> Self {
        Self {
            name: "Dungeon Generator".to_string(),
            gen_type: ProcGenType::Dungeon,
            description: "Generate room-based dungeon layouts with corridors, traps, and loot.".to_string(),
            default_config: ProcGenConfig {
                name: "Dungeon".to_string(),
                gen_type: ProcGenType::Dungeon,
                seed: 0,
                size: [200.0, 20.0, 200.0],
                density: 0.6,
                complexity: 0.5,
                custom_params: [
                    ("min_rooms".to_string(), 5.0),
                    ("max_rooms".to_string(), 15.0),
                    ("corridor_width".to_string(), 3.0),
                    ("trap_density".to_string(), 0.2),
                    ("loot_density".to_string(), 0.3),
                ]
                .into_iter()
                .collect(),
            },
            preview_image: None,
        }
    }

    pub fn terrain() -> Self {
        Self {
            name: "Terrain Generator".to_string(),
            gen_type: ProcGenType::Terrain,
            description: "Generate heightmap-based terrain with biome distribution.".to_string(),
            default_config: ProcGenConfig {
                name: "Terrain".to_string(),
                gen_type: ProcGenType::Terrain,
                seed: 0,
                size: [500.0, 100.0, 500.0],
                density: 0.5,
                complexity: 0.7,
                custom_params: [
                    ("height_scale".to_string(), 50.0),
                    ("octaves".to_string(), 6.0),
                    ("persistence".to_string(), 0.5),
                    ("lacunarity".to_string(), 2.0),
                ]
                .into_iter()
                .collect(),
            },
            preview_image: None,
        }
    }

    pub fn city() -> Self {
        Self {
            name: "City Generator".to_string(),
            gen_type: ProcGenType::City,
            description: "Generate city layouts with buildings, roads, and landmarks.".to_string(),
            default_config: ProcGenConfig {
                name: "City".to_string(),
                gen_type: ProcGenType::City,
                seed: 0,
                size: [300.0, 50.0, 300.0],
                density: 0.8,
                complexity: 0.6,
                custom_params: [
                    ("building_density".to_string(), 0.7),
                    ("road_width".to_string(), 10.0),
                    ("block_size".to_string(), 50.0),
                    ("landmark_count".to_string(), 3.0),
                ]
                .into_iter()
                .collect(),
            },
            preview_image: None,
        }
    }

    pub fn forest() -> Self {
        Self {
            name: "Forest Generator".to_string(),
            gen_type: ProcGenType::Forest,
            description: "Generate forest areas with trees, undergrowth, and clearings.".to_string(),
            default_config: ProcGenConfig {
                name: "Forest".to_string(),
                gen_type: ProcGenType::Forest,
                seed: 0,
                size: [200.0, 30.0, 200.0],
                density: 0.7,
                complexity: 0.4,
                custom_params: [
                    ("tree_density".to_string(), 0.6),
                    ("clearing_size".to_string(), 20.0),
                    ("undergrowth_density".to_string(), 0.4),
                    ("tree_height_min".to_string(), 5.0),
                    ("tree_height_max".to_string(), 20.0),
                ]
                .into_iter()
                .collect(),
            },
            preview_image: None,
        }
    }

    pub fn quest() -> Self {
        Self {
            name: "Quest Generator".to_string(),
            gen_type: ProcGenType::Quest,
            description: "Generate dynamic quests with objectives, rewards, and story elements.".to_string(),
            default_config: ProcGenConfig {
                name: "Quest".to_string(),
                gen_type: ProcGenType::Quest,
                seed: 0,
                size: [1.0, 1.0, 1.0],
                density: 0.5,
                complexity: 0.5,
                custom_params: [
                    ("min_objectives".to_string(), 2.0),
                    ("max_objectives".to_string(), 5.0),
                    ("reward_tier".to_string(), 1.0),
                    ("difficulty".to_string(), 0.5),
                ]
                .into_iter()
                .collect(),
            },
            preview_image: None,
        }
    }
}

pub struct ProcGenSystem {
    templates: Vec<ProcGenTemplate>,
    last_seed: u64,
}

impl ProcGenSystem {
    pub fn new() -> Self {
        Self {
            templates: vec![
                ProcGenTemplate::dungeon(),
                ProcGenTemplate::terrain(),
                ProcGenTemplate::city(),
                ProcGenTemplate::forest(),
                ProcGenTemplate::quest(),
            ],
            last_seed: 0,
        }
    }

    pub fn templates(&self) -> &[ProcGenTemplate] {
        &self.templates
    }

    pub fn generate(&mut self, config: &ProcGenConfig) -> ProcGenResult {
        self.last_seed = config.seed;
        
        match config.gen_type {
            ProcGenType::Dungeon => self.generate_dungeon(config),
            ProcGenType::Terrain => self.generate_terrain(config),
            ProcGenType::City => self.generate_city(config),
            ProcGenType::Forest => self.generate_forest(config),
            ProcGenType::Village => self.generate_village(config),
            ProcGenType::Cave => self.generate_cave(config),
            ProcGenType::SpaceStation => self.generate_space_station(config),
            ProcGenType::Quest => self.generate_quest(config),
        }
    }

    fn generate_dungeon(&self, config: &ProcGenConfig) -> ProcGenResult {
        let mut rng = rng_from_seed(config.seed);
        let mut rooms = Vec::new();
        let mut entities = Vec::new();
        
        let min_rooms = config.custom_params.get("min_rooms").copied().unwrap_or(5.0) as u32;
        let max_rooms = config.custom_params.get("max_rooms").copied().unwrap_or(15.0) as u32;
        let num_rooms = min_rooms + (rng.next_u32() % (max_rooms - min_rooms + 1));
        
        let corridor_width = config.custom_params.get("corridor_width").copied().unwrap_or(3.0);
        let trap_density = config.custom_params.get("trap_density").copied().unwrap_or(0.2);
        let loot_density = config.custom_params.get("loot_density").copied().unwrap_or(0.3);
        
        let mut room_positions: Vec<[f32; 2]> = Vec::new();
        let room_types = ["Entrance", "Hall", "Chamber", "Treasury", "Boss", "Shrine"];
        
        for i in 0..num_rooms {
            let x = (rng.next_u32() % 1000) as f32 / 1000.0 * config.size[0] - config.size[0] / 2.0;
            let z = (rng.next_u32() % 1000) as f32 / 1000.0 * config.size[2] - config.size[2] / 2.0;
            let width = 10.0 + (rng.next_u32() % 20) as f32;
            let depth = 10.0 + (rng.next_u32() % 20) as f32;
            
            let room_type = if i == 0 {
                "Entrance".to_string()
            } else if i == num_rooms - 1 {
                "Boss".to_string()
            } else {
                room_types[(rng.next_u32() as usize) % room_types.len()].to_string()
            };
            
            room_positions.push([x, z]);
            
            let mut room_entities = Vec::new();
            
            if rng.next_u32() as f32 / u32::MAX as f32 < trap_density {
                room_entities.push(GeneratedEntity {
                    entity_type: "Trap".to_string(),
                    position: [x + (rng.next_u32() % 10) as f32 - 5.0, 0.0, z + (rng.next_u32() % 10) as f32 - 5.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0, 1.0, 1.0],
                    properties: [("trap_type".to_string(), "Spike".to_string())].into_iter().collect(),
                });
            }
            
            if rng.next_u32() as f32 / u32::MAX as f32 < loot_density {
                room_entities.push(GeneratedEntity {
                    entity_type: "LootChest".to_string(),
                    position: [x + width / 2.0 - 2.0, 0.0, z + depth / 2.0 - 2.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0, 1.0, 1.0],
                    properties: [("tier".to_string(), ((rng.next_u32() % 3) + 1).to_string())].into_iter().collect(),
                });
            }
            
            rooms.push(GeneratedRoom {
                id: i,
                position: [x, z],
                size: [width, depth],
                room_type,
                connections: Vec::new(),
                entities: room_entities,
            });
        }
        
        let mut connections: Vec<(u32, u32)> = Vec::new();
        for i in 1..num_rooms {
            let mut min_dist = f32::MAX;
            let mut nearest = 0;
            for j in 0..i {
                let dx = room_positions[i as usize][0] - room_positions[j as usize][0];
                let dz = room_positions[i as usize][1] - room_positions[j as usize][1];
                let dist = (dx * dx + dz * dz).sqrt();
                if dist < min_dist {
                    min_dist = dist;
                    nearest = j;
                }
            }
            connections.push((i, nearest));
        }
        
        for (a, b) in &connections {
            rooms[*a as usize].connections.push(*b);
            rooms[*b as usize].connections.push(*a);
        }
        
        let spawn_points = vec![
            [room_positions[0][0], 1.0, room_positions[0][1]],
        ];
        
        ProcGenResult {
            config: config.clone(),
            rooms,
            entities,
            paths: Vec::new(),
            spawn_points,
        }
    }

    fn generate_terrain(&self, config: &ProcGenConfig) -> ProcGenResult {
        let mut rng = rng_from_seed(config.seed);
        let mut entities = Vec::new();
        
        let height_scale = config.custom_params.get("height_scale").copied().unwrap_or(50.0);
        let num_trees = (config.size[0] * config.size[2] * 0.001 * config.density) as u32;
        
        for _ in 0..num_trees {
            let x = (rng.next_u32() % 1000) as f32 / 1000.0 * config.size[0] - config.size[0] / 2.0;
            let z = (rng.next_u32() % 1000) as f32 / 1000.0 * config.size[2] - config.size[2] / 2.0;
            let y = Self::noise_height(x, z, config.seed, height_scale);
            
            entities.push(GeneratedEntity {
                entity_type: "Tree".to_string(),
                position: [x, y, z],
                rotation: [0.0, (rng.next_u32() % 360) as f32 / 180.0 * std::f32::consts::PI, 0.0, 1.0].map(|v| v),
                scale: [1.0, 1.0 + (rng.next_u32() % 50) as f32 / 100.0, 1.0],
                properties: HashMap::new(),
            });
        }
        
        let spawn_points = vec![[0.0, Self::noise_height(0.0, 0.0, config.seed, height_scale), 0.0]];
        
        ProcGenResult {
            config: config.clone(),
            rooms: Vec::new(),
            entities,
            paths: Vec::new(),
            spawn_points,
        }
    }

    fn noise_height(x: f32, z: f32, seed: u64, scale: f32) -> f32 {
        let freq = 0.01;
        let mut h = 0.0;
        let mut amp = 1.0;
        let mut f = freq;
        
        for _ in 0..4 {
            let nx = (x * f).sin() * seed as f32 * 0.0001;
            let nz = (z * f).cos() * seed as f32 * 0.0001;
            h += (nx + nz) * amp;
            f *= 2.0;
            amp *= 0.5;
        }
        
        h * scale
    }

    fn generate_city(&self, config: &ProcGenConfig) -> ProcGenResult {
        let mut rng = rng_from_seed(config.seed);
        let mut rooms = Vec::new();
        let mut entities = Vec::new();
        
        let block_size = config.custom_params.get("block_size").copied().unwrap_or(50.0);
        let road_width = config.custom_params.get("road_width").copied().unwrap_or(10.0);
        
        let blocks_x = (config.size[0] / block_size) as u32;
        let blocks_z = (config.size[2] / block_size) as u32;
        
        let mut room_id = 0;
        for bx in 0..blocks_x {
            for bz in 0..blocks_z {
                let x = (bx as f32 - blocks_x as f32 / 2.0) * block_size;
                let z = (bz as f32 - blocks_z as f32 / 2.0) * block_size;
                
                let building_height = 10.0 + (rng.next_u32() % 50) as f32;
                
                let is_intersection = bx % 2 == 0 || bz % 2 == 0;
                
                if !is_intersection {
                    rooms.push(GeneratedRoom {
                        id: room_id,
                        position: [x, z],
                        size: [block_size - road_width, block_size - road_width],
                        room_type: "Building".to_string(),
                        connections: Vec::new(),
                        entities: vec![GeneratedEntity {
                            entity_type: "Building".to_string(),
                            position: [x, building_height / 2.0, z],
                            rotation: [0.0, 0.0, 0.0, 1.0],
                            scale: [block_size - road_width, building_height, block_size - road_width],
                            properties: [("height".to_string(), building_height.to_string())].into_iter().collect(),
                        }],
                    });
                    room_id += 1;
                }
            }
        }
        
        let spawn_points = vec![[0.0, 1.0, 0.0]];
        
        ProcGenResult {
            config: config.clone(),
            rooms,
            entities,
            paths: Vec::new(),
            spawn_points,
        }
    }

    fn generate_forest(&self, config: &ProcGenConfig) -> ProcGenResult {
        let mut rng = rng_from_seed(config.seed);
        let mut entities = Vec::new();
        
        let tree_density = config.custom_params.get("tree_density").copied().unwrap_or(0.6);
        let tree_height_min = config.custom_params.get("tree_height_min").copied().unwrap_or(5.0);
        let tree_height_max = config.custom_params.get("tree_height_max").copied().unwrap_or(20.0);
        let undergrowth_density = config.custom_params.get("undergrowth_density").copied().unwrap_or(0.4);
        
        let area = config.size[0] * config.size[2];
        let num_trees = (area * tree_density * 0.01) as u32;
        
        for _ in 0..num_trees {
            let x = (rng.next_u32() % 1000) as f32 / 1000.0 * config.size[0] - config.size[0] / 2.0;
            let z = (rng.next_u32() % 1000) as f32 / 1000.0 * config.size[2] - config.size[2] / 2.0;
            let height = tree_height_min + (rng.next_u32() as f32 / u32::MAX as f32) * (tree_height_max - tree_height_min);
            
            entities.push(GeneratedEntity {
                entity_type: "Tree".to_string(),
                position: [x, height / 2.0, z],
                rotation: [0.0, (rng.next_u32() % 360) as f32 * std::f32::consts::PI / 180.0, 0.0, 1.0],
                scale: [1.0, height / 10.0, 1.0],
                properties: [("tree_type".to_string(), ((rng.next_u32() % 3) + 1).to_string())].into_iter().collect(),
            });
            
            if rng.next_u32() as f32 / u32::MAX as f32 < undergrowth_density {
                entities.push(GeneratedEntity {
                    entity_type: "Undergrowth".to_string(),
                    position: [x + (rng.next_u32() % 5) as f32 - 2.5, 0.5, z + (rng.next_u32() % 5) as f32 - 2.5],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [0.5 + (rng.next_u32() % 10) as f32 / 10.0, 0.5, 0.5 + (rng.next_u32() % 10) as f32 / 10.0],
                    properties: HashMap::new(),
                });
            }
        }
        
        let spawn_points = vec![[0.0, 0.0, 0.0]];
        
        ProcGenResult {
            config: config.clone(),
            rooms: Vec::new(),
            entities,
            paths: Vec::new(),
            spawn_points,
        }
    }

    fn generate_village(&self, config: &ProcGenConfig) -> ProcGenResult {
        self.generate_settlement(config, "Village", 5..15)
    }

    fn generate_cave(&self, config: &ProcGenConfig) -> ProcGenResult {
        let mut rng = rng_from_seed(config.seed);
        let mut rooms = Vec::new();
        let mut entities = Vec::new();
        
        let num_chambers = 3 + (rng.next_u32() % 5);
        
        for i in 0..num_chambers {
            let x = (rng.next_u32() % 1000) as f32 / 1000.0 * config.size[0] - config.size[0] / 2.0;
            let z = (rng.next_u32() % 1000) as f32 / 1000.0 * config.size[2] - config.size[2] / 2.0;
            let radius = 5.0 + (rng.next_u32() % 15) as f32;
            
            rooms.push(GeneratedRoom {
                id: i,
                position: [x, z],
                size: [radius * 2.0, radius * 2.0],
                room_type: if i == 0 { "Entrance".to_string() } else { "Chamber".to_string() },
                connections: if i > 0 { vec![i - 1] } else { Vec::new() },
                entities: Vec::new(),
            });
            
            if rng.next_u32() % 3 == 0 {
                entities.push(GeneratedEntity {
                    entity_type: "Crystal".to_string(),
                    position: [x + (rng.next_u32() % 5) as f32 - 2.5, 0.0, z + (rng.next_u32() % 5) as f32 - 2.5],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0, 1.0, 1.0],
                    properties: [("color".to_string(), "Blue".to_string())].into_iter().collect(),
                });
            }
        }
        
        ProcGenResult {
            config: config.clone(),
            rooms,
            entities,
            paths: Vec::new(),
            spawn_points: vec![[0.0, 0.0, 0.0]],
        }
    }

    fn generate_space_station(&self, config: &ProcGenConfig) -> ProcGenResult {
        let mut rng = rng_from_seed(config.seed);
        let mut rooms = Vec::new();
        
        let num_modules = 5 + (rng.next_u32() % 10);
        let module_types = ["Bridge", "Engineering", "Cargo", "Quarters", "Medical", "Research"];
        
        for i in 0..num_modules {
            let angle = i as f32 * std::f32::consts::TAU / num_modules as f32;
            let radius = 20.0 + (rng.next_u32() % 20) as f32;
            let x = angle.cos() * radius;
            let z = angle.sin() * radius;
            
            rooms.push(GeneratedRoom {
                id: i,
                position: [x, z],
                size: [10.0, 10.0],
                room_type: module_types[(rng.next_u32() as usize) % module_types.len()].to_string(),
                connections: vec![(i + 1) % num_modules],
                entities: Vec::new(),
            });
        }
        
        ProcGenResult {
            config: config.clone(),
            rooms,
            entities: Vec::new(),
            paths: Vec::new(),
            spawn_points: vec![[0.0, 0.0, 0.0]],
        }
    }

    fn generate_quest(&self, config: &ProcGenConfig) -> ProcGenResult {
        let mut rng = rng_from_seed(config.seed);
        
        let min_objectives = config.custom_params.get("min_objectives").copied().unwrap_or(2.0) as u32;
        let max_objectives = config.custom_params.get("max_objectives").copied().unwrap_or(5.0) as u32;
        let num_objectives = min_objectives + (rng.next_u32() % (max_objectives - min_objectives + 1));
        
        let objective_types = ["Kill", "Collect", "Escort", "Explore", "Defend", "Retrieve"];
        let quest_names = ["Lost Artifact", "Missing Person", "Monster Hunt", "Supply Run", "Rescue Mission"];
        
        let quest_name = quest_names[(rng.next_u32() as usize) % quest_names.len()];
        let mut objectives = Vec::new();
        
        for i in 0..num_objectives {
            let obj_type = objective_types[(rng.next_u32() as usize) % objective_types.len()];
            objectives.push(GeneratedEntity {
                entity_type: "QuestObjective".to_string(),
                position: [i as f32 * 10.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
                properties: [
                    ("type".to_string(), obj_type.to_string()),
                    ("quest".to_string(), quest_name.to_string()),
                ]
                .into_iter()
                .collect(),
            });
        }
        
        ProcGenResult {
            config: config.clone(),
            rooms: Vec::new(),
            entities: objectives,
            paths: Vec::new(),
            spawn_points: vec![[0.0, 0.0, 0.0]],
        }
    }

    fn generate_settlement(&self, config: &ProcGenConfig, settlement_type: &str, building_range: std::ops::Range<u32>) -> ProcGenResult {
        let mut rng = rng_from_seed(config.seed);
        let mut rooms = Vec::new();
        let mut entities = Vec::new();
        
        let num_buildings = building_range.start + (rng.next_u32() % (building_range.end - building_range.start));
        
        let building_types = ["House", "Shop", "Inn", "Blacksmith", "Temple", "Well"];
        
        for i in 0..num_buildings {
            let x = (rng.next_u32() % 1000) as f32 / 1000.0 * config.size[0] - config.size[0] / 2.0;
            let z = (rng.next_u32() % 1000) as f32 / 1000.0 * config.size[2] - config.size[2] / 2.0;
            let building_type = building_types[(rng.next_u32() as usize) % building_types.len()];
            
            rooms.push(GeneratedRoom {
                id: i,
                position: [x, z],
                size: [8.0, 8.0],
                room_type: building_type.to_string(),
                connections: Vec::new(),
                entities: Vec::new(),
            });
            
            entities.push(GeneratedEntity {
                entity_type: "Building".to_string(),
                position: [x, 4.0, z],
                rotation: [0.0, (rng.next_u32() % 4) as f32 * std::f32::consts::FRAC_PI_2, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
                properties: [("building_type".to_string(), building_type.to_string())].into_iter().collect(),
            });
        }
        
        ProcGenResult {
            config: config.clone(),
            rooms,
            entities,
            paths: Vec::new(),
            spawn_points: vec![[0.0, 0.0, 0.0]],
        }
    }
}

impl Default for ProcGenSystem {
    fn default() -> Self {
        Self::new()
    }
}

mod private {
    pub struct Rng(pub u64);
    
    impl Rng {
        pub fn next_u32(&mut self) -> u32 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            (self.0.wrapping_mul(0x2545F4914F6CDD1D) >> 32) as u32
        }
    }
}

fn rng_from_seed(seed: u64) -> private::Rng {
    private::Rng(if seed == 0 { 1 } else { seed })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_gen_config_default() {
        let config = ProcGenConfig::default();
        assert_eq!(config.gen_type, ProcGenType::Dungeon);
    }

    #[test]
    fn proc_gen_system_creation() {
        let system = ProcGenSystem::new();
        assert!(!system.templates().is_empty());
    }

    #[test]
    fn dungeon_generation() {
        let mut system = ProcGenSystem::new();
        let config = ProcGenConfig {
            gen_type: ProcGenType::Dungeon,
            seed: 42,
            ..Default::default()
        };
        let result = system.generate(&config);
        assert!(!result.rooms.is_empty());
    }

    #[test]
    fn terrain_generation() {
        let mut system = ProcGenSystem::new();
        let config = ProcGenConfig {
            gen_type: ProcGenType::Terrain,
            seed: 42,
            ..Default::default()
        };
        let result = system.generate(&config);
        assert!(!result.entities.is_empty());
    }

    #[test]
    fn city_generation() {
        let mut system = ProcGenSystem::new();
        let config = ProcGenConfig {
            gen_type: ProcGenType::City,
            seed: 42,
            size: [100.0, 20.0, 100.0],
            ..Default::default()
        };
        let result = system.generate(&config);
        assert!(!result.rooms.is_empty());
    }

    #[test]
    fn forest_generation() {
        let mut system = ProcGenSystem::new();
        let config = ProcGenConfig {
            gen_type: ProcGenType::Forest,
            seed: 42,
            ..Default::default()
        };
        let result = system.generate(&config);
        assert!(!result.entities.is_empty());
    }

    #[test]
    fn seed_reproducibility() {
        let mut system = ProcGenSystem::new();
        let config = ProcGenConfig {
            gen_type: ProcGenType::Dungeon,
            seed: 12345,
            ..Default::default()
        };
        let result1 = system.generate(&config);
        let result2 = system.generate(&config);
        assert_eq!(result1.rooms.len(), result2.rooms.len());
    }
}
