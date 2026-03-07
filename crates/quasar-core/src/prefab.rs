//! Prefab system — serializable entity templates.
//!
//! A `Prefab` is a reusable blueprint for one or more entities.  Prefabs
//! are stored as JSON and can be instantiated at runtime, spawning a fresh
//! set of entities with cloned component data.
//!
//! The prefab format builds on [`crate::scene_serde::EntityData`] and adds
//! optional physics / audio configuration as serde-friendly primitives.

use crate::ecs::{Entity, World};
use crate::scene_serde::EntityData;
use quasar_math::Transform;
use serde::{Deserialize, Serialize};
use std::path::Path;

// ---------------------------------------------------------------------------
// Prefab data model
// ---------------------------------------------------------------------------

/// A key-value pair for a generic component property.
/// This is intentionally stringly-typed so that user components can be
/// serialized without requiring a compile-time type registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefabProperty {
    pub key: String,
    pub value: serde_json::Value,
}

/// A single entity template within a prefab.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefabEntity {
    /// Human-readable tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Local transform.
    pub transform: Transform,
    /// Mesh shape tag (e.g. "Cube", "Sphere").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh_shape: Option<String>,
    /// Index of the parent entity inside `Prefab::entities` (root = `None`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<usize>,
    /// Arbitrary additional properties that game-specific systems can read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<PrefabProperty>,
}

/// A reusable entity blueprint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prefab {
    /// Prefab name (unique identifier).
    pub name: String,
    /// Ordered list of entity templates. Parent indices reference this vec.
    pub entities: Vec<PrefabEntity>,
}

impl Prefab {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            entities: Vec::new(),
        }
    }

    /// Add a root entity and return its index.
    pub fn add_entity(&mut self, entity: PrefabEntity) -> usize {
        let idx = self.entities.len();
        self.entities.push(entity);
        idx
    }

    /// Serialize to pretty JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Load a prefab from a file.
    pub fn load(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        Self::from_json(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Save a prefab to a file.
    pub fn save(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let json = self
            .to_json()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    /// Convert to `SceneData` for interop with the scene serialization layer.
    pub fn to_scene_data(&self) -> crate::scene_serde::SceneData {
        let mut data = crate::scene_serde::SceneData::new(&self.name);
        // Build a map from template index → children list.
        let mut children_map: std::collections::HashMap<usize, Vec<usize>> =
            std::collections::HashMap::new();
        for (i, pe) in self.entities.iter().enumerate() {
            if let Some(parent) = pe.parent {
                children_map.entry(parent).or_default().push(i);
            }
        }
        for (i, pe) in self.entities.iter().enumerate() {
            data.entities.push(EntityData {
                name: pe.name.clone(),
                transform: pe.transform,
                mesh_shape: pe.mesh_shape.clone(),
                children: children_map.get(&i).cloned().unwrap_or_default(),
            });
        }
        data
    }
}

// ---------------------------------------------------------------------------
// Instantiation
// ---------------------------------------------------------------------------

/// Spawn entities from a prefab into the world. Returns a list of
/// `(Entity, &PrefabEntity)` pairs in the same order as `prefab.entities`,
/// so callers can process mesh_shape and properties.
pub fn instantiate_prefab<'a>(world: &mut World, prefab: &'a Prefab) -> Vec<(Entity, &'a PrefabEntity)> {
    let mut spawned: Vec<(Entity, &PrefabEntity)> = Vec::with_capacity(prefab.entities.len());

    for pe in &prefab.entities {
        let entity = world.spawn();
        world.insert(entity, pe.transform);

        // Insert mesh_shape as a Name-like tag that downstream systems can match.
        if let Some(ref shape) = pe.mesh_shape {
            world.insert(entity, PrefabMeshTag(shape.clone()));
        }

        // Insert properties so game systems can read them.
        if !pe.properties.is_empty() {
            world.insert(entity, PrefabProperties(pe.properties.clone()));
        }

        spawned.push((entity, pe));
    }

    spawned
}

/// Tag component inserted for prefab entities that specify a `mesh_shape`.
#[derive(Debug, Clone)]
pub struct PrefabMeshTag(pub String);

/// Component holding arbitrary prefab properties for game-specific systems.
#[derive(Debug, Clone)]
pub struct PrefabProperties(pub Vec<PrefabProperty>);

// ---------------------------------------------------------------------------
// Prefab Asset
// ---------------------------------------------------------------------------

/// Resource that holds named prefabs.
#[derive(Debug, Clone, Default)]
pub struct PrefabLibrary {
    prefabs: std::collections::HashMap<String, Prefab>,
}

impl PrefabLibrary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, prefab: Prefab) {
        self.prefabs.insert(prefab.name.clone(), prefab);
    }

    pub fn get(&self, name: &str) -> Option<&Prefab> {
        self.prefabs.get(name)
    }

    pub fn remove(&mut self, name: &str) -> Option<Prefab> {
        self.prefabs.remove(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.prefabs.keys().map(|s| s.as_str())
    }
}
