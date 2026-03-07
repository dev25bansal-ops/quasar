//! Save / Load game state.
//!
//! Provides a serializable `GameSave` snapshot that captures entity
//! transforms and names from the ECS world plus any user-provided
//! key-value metadata.  The snapshot can be written to disk as JSON
//! and loaded back later.
//!
//! This module intentionally keeps the format simple and extensible.
//! Game-specific components should be captured using the `custom_data`
//! escape hatch, which stores arbitrary `serde_json::Value` per entity.

use crate::ecs::{Entity, World};
use crate::scene::SceneGraph;
use crate::scene_serde::{EntityData, SceneData};
use quasar_math::Transform;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

/// Per-entity snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedEntity {
    /// Entity index at save time (used to rebuild references).
    pub index: u32,
    /// Optional human-readable name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Transform at save time.
    pub transform: Transform,
    /// Children indices in the `GameSave::entities` vec.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<usize>,
    /// Arbitrary per-entity data that game code can populate.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom_data: HashMap<String, serde_json::Value>,
}

/// Metadata attached to a save file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveMeta {
    /// Descriptive name for the save.
    pub slot_name: String,
    /// Timestamp (ISO-8601 or free-form string).
    #[serde(default)]
    pub timestamp: String,
    /// Arbitrary key-value pairs (playtime, chapter, etc.).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub extra: HashMap<String, String>,
}

/// Top-level game save structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSave {
    pub meta: SaveMeta,
    pub entities: Vec<SavedEntity>,
}

impl GameSave {
    // ------------------------------------------------------------------
    // Serialization
    // ------------------------------------------------------------------

    /// Serialize to pretty JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Write to a file.
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let json = self
            .to_json()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    /// Read from a file.
    pub fn load_from_file(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        Self::from_json(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Convenience: convert into `SceneData` for the existing scene pipeline.
    pub fn to_scene_data(&self) -> SceneData {
        let mut sd = SceneData::new(&self.meta.slot_name);
        for se in &self.entities {
            sd.entities.push(EntityData {
                name: se.name.clone(),
                transform: se.transform,
                mesh_shape: None,
                children: se.children.clone(),
            });
        }
        sd
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Capture a snapshot of all entities that have a `Transform` in `world`.
///
/// If a `SceneGraph` resource is available the entity names are read from
/// there; otherwise names will be `None`.
///
/// The returned `GameSave` uses a minimal `SaveMeta`; callers should
/// populate `meta.slot_name` and `meta.timestamp` as desired.
pub fn capture_game_save(world: &World) -> GameSave {
    let transforms: Vec<(Entity, Transform)> = world
        .query::<Transform>()
        .into_iter()
        .map(|(e, t)| (e, *t))
        .collect();

    let graph = world.resource::<SceneGraph>();

    let entities: Vec<SavedEntity> = transforms
        .iter()
        .map(|(e, t)| {
            let name = graph.and_then(|g| g.name(*e).map(|s| s.to_string()));
            SavedEntity {
                index: e.index(),
                name,
                transform: *t,
                children: Vec::new(),
                custom_data: HashMap::new(),
            }
        })
        .collect();

    GameSave {
        meta: SaveMeta {
            slot_name: String::new(),
            timestamp: String::new(),
            extra: HashMap::new(),
        },
        entities,
    }
}

/// Load a `GameSave` into a fresh world, spawning entities with their saved
/// transforms.  Returns `(Entity, &SavedEntity)` pairs so callers can
/// process `custom_data` and other per-entity fields.
pub fn load_game_save<'a>(world: &mut World, save: &'a GameSave) -> Vec<(Entity, &'a SavedEntity)> {
    let mut spawned: Vec<(Entity, &SavedEntity)> = Vec::with_capacity(save.entities.len());

    for se in &save.entities {
        let entity = world.spawn();
        world.insert(entity, se.transform);
        spawned.push((entity, se));
    }

    spawned
}
