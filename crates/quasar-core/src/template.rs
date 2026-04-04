//! Game project templates.
//!
//! Provides:
//! - Zero-config project templates
//! - Built-in game templates (FPS, RPG, Platformer, RTS)
//! - One-click prototype to production workflow

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Project template metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectTemplate {
    /// Template ID.
    pub id: String,
    /// Template name.
    pub name: String,
    /// Template description.
    pub description: String,
    /// Template category.
    pub category: TemplateCategory,
    /// Preview image path.
    pub preview_image: Option<String>,
    /// Required features.
    pub features: Vec<String>,
    /// Default systems.
    pub systems: Vec<SystemConfig>,
    /// Default components.
    pub components: Vec<ComponentConfig>,
    /// Default assets.
    pub assets: Vec<AssetConfig>,
    /// Default scenes.
    pub scenes: Vec<SceneConfig>,
    /// Networking support.
    pub multiplayer: bool,
    /// Template version.
    pub version: String,
}

impl ProjectTemplate {
    /// Create an FPS template.
    pub fn fps() -> Self {
        Self {
            id: "fps".to_string(),
            name: "First-Person Shooter".to_string(),
            description: "First-person shooter template with weapons, enemies, and health system"
                .to_string(),
            category: TemplateCategory::Action,
            preview_image: Some("templates/fps_preview.png".to_string()),
            features: vec![
                "shooter-controls".to_string(),
                "weapon-system".to_string(),
                "enemy-ai".to_string(),
                "health-damage".to_string(),
                "ammo-pickups".to_string(),
            ],
            systems: vec![
                SystemConfig {
                    name: "InputSystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "MovementSystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "WeaponSystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "HealthSystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "AISystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "PhysicsSystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "AudioSystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "UISystem".to_string(),
                    enabled: true,
                },
            ],
            components: vec![
                ComponentConfig {
                    name: "PlayerController".to_string(),
                    config: HashMap::new(),
                },
                ComponentConfig {
                    name: "FirstPersonCamera".to_string(),
                    config: HashMap::new(),
                },
                ComponentConfig {
                    name: "Health".to_string(),
                    config: [("max_health".to_string(), "100".to_string())].into(),
                },
                ComponentConfig {
                    name: "Inventory".to_string(),
                    config: HashMap::new(),
                },
            ],
            assets: vec![
                AssetConfig {
                    id: "player".to_string(),
                    path: "models/player.fbx".to_string(),
                    asset_type: "model".to_string(),
                },
                AssetConfig {
                    id: "rifle".to_string(),
                    path: "models/rifle.fbx".to_string(),
                    asset_type: "model".to_string(),
                },
                AssetConfig {
                    id: "enemy".to_string(),
                    path: "models/enemy.fbx".to_string(),
                    asset_type: "model".to_string(),
                },
            ],
            scenes: vec![SceneConfig {
                name: "main".to_string(),
                path: "scenes/main.scene".to_string(),
            }],
            multiplayer: true,
            version: "1.0.0".to_string(),
        }
    }

    /// Create an RPG template.
    pub fn rpg() -> Self {
        Self {
            id: "rpg".to_string(),
            name: "Role-Playing Game".to_string(),
            description: "RPG template with quests, inventory, dialog, and character progression"
                .to_string(),
            category: TemplateCategory::RPG,
            preview_image: Some("templates/rpg_preview.png".to_string()),
            features: vec![
                "third-person-camera".to_string(),
                "inventory-system".to_string(),
                "quest-system".to_string(),
                "dialog-system".to_string(),
                "character-stats".to_string(),
                "skill-tree".to_string(),
                "save-load".to_string(),
            ],
            systems: vec![
                SystemConfig {
                    name: "InputSystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "MovementSystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "QuestSystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "DialogSystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "InventorySystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "CombatSystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "SaveLoadSystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "UISystem".to_string(),
                    enabled: true,
                },
            ],
            components: vec![
                ComponentConfig {
                    name: "PlayerController".to_string(),
                    config: HashMap::new(),
                },
                ComponentConfig {
                    name: "ThirdPersonCamera".to_string(),
                    config: HashMap::new(),
                },
                ComponentConfig {
                    name: "CharacterStats".to_string(),
                    config: HashMap::new(),
                },
                ComponentConfig {
                    name: "Inventory".to_string(),
                    config: HashMap::new(),
                },
                ComponentConfig {
                    name: "QuestTracker".to_string(),
                    config: HashMap::new(),
                },
            ],
            assets: vec![
                AssetConfig {
                    id: "player".to_string(),
                    path: "models/hero.fbx".to_string(),
                    asset_type: "model".to_string(),
                },
                AssetConfig {
                    id: "npc".to_string(),
                    path: "models/npc.fbx".to_string(),
                    asset_type: "model".to_string(),
                },
            ],
            scenes: vec![
                SceneConfig {
                    name: "town".to_string(),
                    path: "scenes/town.scene".to_string(),
                },
                SceneConfig {
                    name: "dungeon".to_string(),
                    path: "scenes/dungeon.scene".to_string(),
                },
            ],
            multiplayer: false,
            version: "1.0.0".to_string(),
        }
    }

    /// Create a Platformer template.
    pub fn platformer() -> Self {
        Self {
            id: "platformer".to_string(),
            name: "2D/3D Platformer".to_string(),
            description: "Platformer template with jumping, collectibles, and level progression"
                .to_string(),
            category: TemplateCategory::Action,
            preview_image: Some("templates/platformer_preview.png".to_string()),
            features: vec![
                "platformer-controls".to_string(),
                "jump-physics".to_string(),
                "collectibles".to_string(),
                "checkpoints".to_string(),
                "level-streaming".to_string(),
            ],
            systems: vec![
                SystemConfig {
                    name: "InputSystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "PlatformerMovement".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "CollectibleSystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "CheckpointSystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "LevelStreamingSystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "UISystem".to_string(),
                    enabled: true,
                },
            ],
            components: vec![
                ComponentConfig {
                    name: "PlatformerController".to_string(),
                    config: HashMap::new(),
                },
                ComponentConfig {
                    name: "Jump".to_string(),
                    config: [("jump_force".to_string(), "10.0".to_string())].into(),
                },
                ComponentConfig {
                    name: "Collectible".to_string(),
                    config: HashMap::new(),
                },
                ComponentConfig {
                    name: "Checkpoint".to_string(),
                    config: HashMap::new(),
                },
            ],
            assets: vec![
                AssetConfig {
                    id: "player".to_string(),
                    path: "models/character.fbx".to_string(),
                    asset_type: "model".to_string(),
                },
                AssetConfig {
                    id: "coin".to_string(),
                    path: "models/coin.fbx".to_string(),
                    asset_type: "model".to_string(),
                },
            ],
            scenes: vec![SceneConfig {
                name: "level_1".to_string(),
                path: "scenes/level_1.scene".to_string(),
            }],
            multiplayer: true,
            version: "1.0.0".to_string(),
        }
    }

    /// Create an RTS template.
    pub fn rts() -> Self {
        Self {
            id: "rts".to_string(),
            name: "Real-Time Strategy".to_string(),
            description: "RTS template with unit selection, resource gathering, and base building"
                .to_string(),
            category: TemplateCategory::Strategy,
            preview_image: Some("templates/rts_preview.png".to_string()),
            features: vec![
                "rts-controls".to_string(),
                "unit-selection".to_string(),
                "resource-system".to_string(),
                "building-system".to_string(),
                "fog-of-war".to_string(),
                "minimap".to_string(),
            ],
            systems: vec![
                SystemConfig {
                    name: "InputSystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "RTSCamera".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "UnitSelectionSystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "ResourceSystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "BuildingSystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "FogOfWarSystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "AISystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "UISystem".to_string(),
                    enabled: true,
                },
            ],
            components: vec![
                ComponentConfig {
                    name: "Selectable".to_string(),
                    config: HashMap::new(),
                },
                ComponentConfig {
                    name: "RTSUnit".to_string(),
                    config: HashMap::new(),
                },
                ComponentConfig {
                    name: "ResourceGatherer".to_string(),
                    config: HashMap::new(),
                },
                ComponentConfig {
                    name: "Building".to_string(),
                    config: HashMap::new(),
                },
            ],
            assets: vec![
                AssetConfig {
                    id: "worker".to_string(),
                    path: "models/worker.fbx".to_string(),
                    asset_type: "model".to_string(),
                },
                AssetConfig {
                    id: "soldier".to_string(),
                    path: "models/soldier.fbx".to_string(),
                    asset_type: "model".to_string(),
                },
                AssetConfig {
                    id: "building".to_string(),
                    path: "models/building.fbx".to_string(),
                    asset_type: "model".to_string(),
                },
            ],
            scenes: vec![SceneConfig {
                name: "skirmish".to_string(),
                path: "scenes/skirmish.scene".to_string(),
            }],
            multiplayer: true,
            version: "1.0.0".to_string(),
        }
    }

    /// Create an empty template.
    pub fn empty() -> Self {
        Self {
            id: "empty".to_string(),
            name: "Empty Project".to_string(),
            description: "Empty project with minimal setup".to_string(),
            category: TemplateCategory::Empty,
            preview_image: None,
            features: vec![],
            systems: vec![
                SystemConfig {
                    name: "InputSystem".to_string(),
                    enabled: true,
                },
                SystemConfig {
                    name: "RenderSystem".to_string(),
                    enabled: true,
                },
            ],
            components: vec![],
            assets: vec![],
            scenes: vec![SceneConfig {
                name: "main".to_string(),
                path: "scenes/main.scene".to_string(),
            }],
            multiplayer: false,
            version: "1.0.0".to_string(),
        }
    }
}

/// Template category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemplateCategory {
    Empty,
    Action,
    RPG,
    Strategy,
    Puzzle,
    Simulation,
    Sports,
    Racing,
}

/// System configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    pub name: String,
    pub enabled: bool,
}

/// Component configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentConfig {
    pub name: String,
    pub config: HashMap<String, String>,
}

/// Asset configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetConfig {
    pub id: String,
    pub path: String,
    pub asset_type: String,
}

/// Scene configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneConfig {
    pub name: String,
    pub path: String,
}

/// Template manager.
pub struct TemplateManager {
    /// All available templates.
    pub templates: HashMap<String, ProjectTemplate>,
}

impl TemplateManager {
    pub fn new() -> Self {
        let mut templates = HashMap::new();

        let fps = ProjectTemplate::fps();
        templates.insert(fps.id.clone(), fps);

        let rpg = ProjectTemplate::rpg();
        templates.insert(rpg.id.clone(), rpg);

        let platformer = ProjectTemplate::platformer();
        templates.insert(platformer.id.clone(), platformer);

        let rts = ProjectTemplate::rts();
        templates.insert(rts.id.clone(), rts);

        let empty = ProjectTemplate::empty();
        templates.insert(empty.id.clone(), empty);

        Self { templates }
    }

    /// Get template by ID.
    pub fn get(&self, id: &str) -> Option<&ProjectTemplate> {
        self.templates.get(id)
    }

    /// Get all templates.
    pub fn all(&self) -> Vec<&ProjectTemplate> {
        self.templates.values().collect()
    }

    /// Get templates by category.
    pub fn by_category(&self, category: TemplateCategory) -> Vec<&ProjectTemplate> {
        self.templates
            .values()
            .filter(|t| t.category == category)
            .collect()
    }

    /// Get multiplayer templates.
    pub fn multiplayer(&self) -> Vec<&ProjectTemplate> {
        self.templates.values().filter(|t| t.multiplayer).collect()
    }

    /// Create a new project from template.
    pub fn create_project(&self, template_id: &str, project_name: &str) -> Option<ProjectConfig> {
        let template = self.get(template_id)?;

        Some(ProjectConfig {
            name: project_name.to_string(),
            template: template_id.to_string(),
            systems: template.systems.clone(),
            components: template.components.clone(),
            assets: template.assets.clone(),
            scenes: template.scenes.clone(),
            settings: ProjectSettings {
                multiplayer: template.multiplayer,
                save_enabled: true,
                debug_mode: true,
            },
        })
    }
}

impl Default for TemplateManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Generated project configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    pub template: String,
    pub systems: Vec<SystemConfig>,
    pub components: Vec<ComponentConfig>,
    pub assets: Vec<AssetConfig>,
    pub scenes: Vec<SceneConfig>,
    pub settings: ProjectSettings,
}

/// Project settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSettings {
    pub multiplayer: bool,
    pub save_enabled: bool,
    pub debug_mode: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_manager_creation() {
        let manager = TemplateManager::new();
        assert!(manager.get("fps").is_some());
        assert!(manager.get("rpg").is_some());
        assert!(manager.get("platformer").is_some());
        assert!(manager.get("rts").is_some());
    }

    #[test]
    fn fps_template() {
        let template = ProjectTemplate::fps();
        assert_eq!(template.id, "fps");
        assert!(template.multiplayer);
        assert!(!template.features.is_empty());
    }

    #[test]
    fn create_project_from_template() {
        let manager = TemplateManager::new();
        let project = manager.create_project("fps", "My FPS Game");

        assert!(project.is_some());
        let project = project.unwrap();
        assert_eq!(project.name, "My FPS Game");
        assert_eq!(project.template, "fps");
    }

    #[test]
    fn multiplayer_templates() {
        let manager = TemplateManager::new();
        let mp_templates = manager.multiplayer();

        assert!(mp_templates.len() >= 3); // FPS, Platformer, RTS
    }
}
