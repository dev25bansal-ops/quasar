//! Scene serialization — save and load scene data from JSON files.
//!
//! Provides [`SceneData`] as a serializable snapshot of entity transforms,
//! names, parent–child hierarchy, and mesh shapes. This is intentionally
//! decoupled from the ECS `World` so it can be round-tripped through JSON
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
    /// Format version — bump when the schema changes.
    #[serde(default = "default_version")]
    pub version: u32,
    /// All entities in depth-first order.
    pub entities: Vec<EntityData>,
}

fn default_version() -> u32 {
    1
}

impl SceneData {
    /// Current schema version.
    pub const CURRENT_VERSION: u32 = 1;

    /// Create an empty scene data container.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: Self::CURRENT_VERSION,
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
        Self::from_json(&json).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
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

    #[test]
    fn integration_spawn_serialize_deserialize_verify() {
        use crate::ecs::World;
        use crate::scene::SceneGraph;

        let mut world = World::new();
        let mut graph = SceneGraph::new();

        let parent = world.spawn();
        world.insert(
            parent,
            Transform::from_position(quasar_math::Vec3::new(10.0, 0.0, 0.0)),
        );
        graph.set_name(parent, "Parent");

        let child1 = world.spawn();
        world.insert(
            child1,
            Transform::from_position(quasar_math::Vec3::new(1.0, 2.0, 3.0)),
        );
        graph.set_name(child1, "Child1");
        graph.set_parent(child1, parent);

        let child2 = world.spawn();
        world.insert(
            child2,
            Transform::from_position(quasar_math::Vec3::new(4.0, 5.0, 6.0)),
        );
        graph.set_name(child2, "Child2");
        graph.set_parent(child2, parent);

        let mut scene_data = SceneData::new("IntegrationTest");

        fn collect_entity_data(
            entity: crate::Entity,
            world: &World,
            graph: &SceneGraph,
            entities: &mut Vec<EntityData>,
        ) {
            let transform = world
                .get::<Transform>(entity)
                .copied()
                .unwrap_or(Transform::IDENTITY);
            let name = graph.name(entity).map(|s| s.to_string());

            let children: Vec<crate::Entity> = graph.children(entity).to_vec();
            let child_start_idx = entities.len() + 1;

            let mesh_shape = None;

            let child_indices: Vec<usize> =
                (0..children.len()).map(|i| child_start_idx + i).collect();

            entities.push(EntityData {
                name,
                transform,
                mesh_shape,
                children: child_indices,
            });

            for child in children {
                collect_entity_data(child, world, graph, entities);
            }
        }

        let roots = graph.roots(&[parent, child1, child2]);
        for root in roots {
            collect_entity_data(root, &world, &graph, &mut scene_data.entities);
        }

        let json = scene_data.to_json().unwrap();
        let loaded = SceneData::from_json(&json).unwrap();

        assert_eq!(loaded.name, "IntegrationTest");
        assert_eq!(loaded.entities.len(), 3);

        let loaded_parent = &loaded.entities[0];
        assert_eq!(loaded_parent.name.as_deref(), Some("Parent"));
        assert!((loaded_parent.transform.position.x - 10.0).abs() < 1e-5);
        assert_eq!(loaded_parent.children.len(), 2);

        let child_indices = &loaded_parent.children;
        for &idx in child_indices {
            let child = &loaded.entities[idx];
            assert!(
                child.name.as_deref() == Some("Child1") || child.name.as_deref() == Some("Child2")
            );
            if child.name.as_deref() == Some("Child1") {
                assert!((child.transform.position.x - 1.0).abs() < 1e-5);
                assert!((child.transform.position.y - 2.0).abs() < 1e-5);
                assert!((child.transform.position.z - 3.0).abs() < 1e-5);
            } else {
                assert!((child.transform.position.x - 4.0).abs() < 1e-5);
                assert!((child.transform.position.y - 5.0).abs() < 1e-5);
                assert!((child.transform.position.z - 6.0).abs() < 1e-5);
            }
        }
    }
}
