//! Scene serialization — save and load scene data from JSON files.
//!
//! Provides [`SceneData`] as a serializable snapshot of entity transforms,
//! names, parent–child hierarchy, and mesh shapes. This is intentionally
//! decoupled from the ECS [`World`] so it can be round-tripped through JSON
//! and later loaded back into a fresh world.

use std::path::Path;

use serde::{Deserialize, Serialize};

use quasar_math::Transform;

/// Serializable description of a single entity in a scene.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityData {
    /// Human-readable name (optional).
    pub name: Option<String>,
    /// Local transform.
    pub transform: Transform,
    /// Mesh shape tag (e.g. "Cube", "Sphere").  Stored as a string so the
    /// scene format is independent of the render crate's `MeshShape` enum.
    pub mesh_shape: Option<String>,
    /// Indices of child entities in the parent [`SceneData::entities`] vec.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<usize>,
}

/// A serializable snapshot of an entire scene.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneData {
    /// Scene name.
    pub name: String,
    /// All entities in depth-first order.
    pub entities: Vec<EntityData>,
}

impl SceneData {
    /// Create an empty scene data container.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            entities: Vec::new(),
        }
    }

    /// Serialize this scene to a pretty-printed JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize a scene from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Save this scene to a file.
    pub fn save(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let json = self
            .to_json()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    /// Load a scene from a file.
    pub fn load(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        Self::from_json(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_json() {
        let mut scene = SceneData::new("TestScene");
        scene.entities.push(EntityData {
            name: Some("Player".into()),
            transform: Transform::IDENTITY,
            mesh_shape: Some("Cube".into()),
            children: vec![1],
        });
        scene.entities.push(EntityData {
            name: Some("Weapon".into()),
            transform: Transform::from_position(quasar_math::Vec3::new(0.0, 1.0, 0.0)),
            mesh_shape: Some("Cylinder".into()),
            children: vec![],
        });

        let json = scene.to_json().unwrap();
        let loaded = SceneData::from_json(&json).unwrap();

        assert_eq!(loaded.name, "TestScene");
        assert_eq!(loaded.entities.len(), 2);
        assert_eq!(loaded.entities[0].name.as_deref(), Some("Player"));
        assert_eq!(loaded.entities[1].name.as_deref(), Some("Weapon"));
        assert_eq!(loaded.entities[0].children, vec![1]);
    }
}
