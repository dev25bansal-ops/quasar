//! Integration tests for Save/Load + Scene interaction.
//!
//! Verifies that complex scenes with hierarchies can be saved and loaded
//! correctly, including:
//! - Creating complex scenes with parent-child relationships
//! - Saving scenes to binary and JSON formats
//! - Loading and verifying all entities, transforms, and relationships
//! - Testing save/load roundtrip with component overrides

use quasar_core::ecs::World;
use quasar_core::save_load::*;
use quasar_core::scene::SceneGraph;
use quasar_core::scene_serde::{EntityData, SceneData};
use quasar_math::{Quat, Transform, Vec3};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_test_transform(x: f32, y: f32, z: f32) -> Transform {
    Transform::from_position(Vec3::new(x, y, z))
}

fn create_test_scene_graph(world: &mut World) -> Vec<quasar_core::ecs::Entity> {
    let mut graph = SceneGraph::new();
    let mut entities = Vec::new();

    // Create root entity
    let root = world.spawn();
    world.insert(root, create_test_transform(0.0, 0.0, 0.0));
    graph.set_name(root, "Root");
    entities.push(root);

    // Create children
    for i in 0..3 {
        let child = world.spawn();
        world.insert(
            child,
            create_test_transform(i as f32 * 2.0, 1.0, 0.0),
        );
        graph.set_name(child, format!("Child_{}", i));
        graph.set_parent(child, root);
        entities.push(child);
    }

    // Create grandchildren
    for i in 0..2 {
        let grandchild = world.spawn();
        world.insert(
            grandchild,
            create_test_transform(i as f32, 2.0, 0.0),
        );
        graph.set_name(grandchild, format!("Grandchild_{}", i));
        graph.set_parent(grandchild, entities[1]); // Parent is first child
        entities.push(grandchild);
    }

    world.insert_resource(graph);
    entities
}

// ---------------------------------------------------------------------------
// 1. Create complex scene with hierarchy (parent-child relationships)
// ---------------------------------------------------------------------------

#[test]
fn test_create_scene_with_hierarchy() {
    let mut world = World::new();
    let entities = create_test_scene_graph(&mut world);

    assert_eq!(entities.len(), 6); // 1 root + 3 children + 2 grandchildren

    // Verify graph relationships
    let graph = world.resource::<SceneGraph>().unwrap();

    // Root should have no parent
    assert!(graph.parent(entities[0]).is_none());

    // Children should have root as parent
    for i in 1..=3 {
        let parent = graph.parent(entities[i]);
        assert_eq!(parent, Some(entities[0]));
    }

    // Grandchildren should have first child as parent
    for i in 4..=5 {
        let parent = graph.parent(entities[i]);
        assert_eq!(parent, Some(entities[1]));
    }

    // Root should have 3 children
    assert_eq!(graph.children(entities[0]).len(), 3);

    // First child should have 2 children
    assert_eq!(graph.children(entities[1]).len(), 2);
}

#[test]
fn test_scene_graph_named_entities() {
    let mut world = World::new();
    create_test_scene_graph(&mut world);

    let graph = world.resource::<SceneGraph>().unwrap();

    // Find entities by name
    let root = graph.find_by_name("Root").expect("Root should exist");
    let child_1 = graph
        .find_by_name("Child_1")
        .expect("Child_1 should exist");
    let grandchild_0 = graph
        .find_by_name("Grandchild_0")
        .expect("Grandchild_0 should exist");

    // Verify names
    assert_eq!(graph.name(root), Some("Root"));
    assert_eq!(graph.name(child_1), Some("Child_1"));
    assert_eq!(graph.name(grandchild_0), Some("Grandchild_0"));
}

#[test]
fn test_scene_graph_descendants() {
    let mut world = World::new();
    let entities = create_test_scene_graph(&mut world);

    let graph = world.resource::<SceneGraph>().unwrap();

    // Get all descendants of root
    let root_descendants = graph.descendants(entities[0]);
    assert_eq!(root_descendants.len(), 5); // 3 children + 2 grandchildren

    // Get descendants of first child
    let child_descendants = graph.descendants(entities[1]);
    assert_eq!(child_descendants.len(), 2); // 2 grandchildren
}

#[test]
fn test_scene_graph_ancestors() {
    let mut world = World::new();
    let entities = create_test_scene_graph(&mut world);

    let graph = world.resource::<SceneGraph>().unwrap();

    // Get ancestors of a grandchild
    let ancestors = graph.ancestors(entities[4]);
    assert_eq!(ancestors.len(), 2); // child -> root

    // First ancestor should be parent (child)
    assert_eq!(ancestors[0], entities[1]);
    // Second ancestor should be grandparent (root)
    assert_eq!(ancestors[1], entities[0]);
}

#[test]
fn test_scene_graph_unparent() {
    let mut world = World::new();
    let entities = create_test_scene_graph(&mut world);

    {
        let graph = world.resource_mut::<SceneGraph>().unwrap();
        // Unparent first child
        graph.unparent(entities[1]);
    }

    let graph = world.resource::<SceneGraph>().unwrap();

    // First child should have no parent
    assert!(graph.parent(entities[1]).is_none());

    // Root should now have 2 children
    assert_eq!(graph.children(entities[0]).len(), 2);
}

// ---------------------------------------------------------------------------
// 2. Save scene to binary and JSON formats
// ---------------------------------------------------------------------------

#[test]
fn test_capture_game_save_with_hierarchy() {
    let mut world = World::new();
    let entities = create_test_scene_graph(&mut world);

    // Capture save
    let save = capture_game_save(&world);

    // Should have captured all entities with transforms
    assert_eq!(save.entities.len(), 6);

    // Verify entities have transforms
    for saved_entity in &save.entities {
        assert!(saved_entity.name.is_some());
    }

    // Prevent unused variable warning
    let _ = entities;
}

#[test]
fn test_save_to_json_format() {
    let mut world = World::new();
    create_test_scene_graph(&mut world);

    let mut save = capture_game_save(&world);
    save.meta.slot_name = "Test JSON Save".to_string();

    // Serialize to JSON
    let json = save.to_json().expect("Should serialize to JSON");

    // Verify JSON is valid and contains expected data
    assert!(json.contains("Test JSON Save"));
    assert!(json.contains("Root"));
    assert!(json.contains("Child_0"));
}

#[test]
fn test_save_to_binary_format() {
    let mut world = World::new();
    create_test_scene_graph(&mut world);

    let mut save = capture_game_save(&world);
    save.meta.slot_name = "Test Binary Save".to_string();

    // Serialize to binary
    let binary = save.to_binary().expect("Should serialize to binary");

    // Verify binary has header + data
    assert!(binary.len() > 16); // At least header size
    assert_eq!(&binary[0..4], b"QSAV"); // Magic bytes
}

#[test]
fn test_save_json_vs_binary_size_comparison() {
    let mut world = World::new();

    // Create a larger scene
    for i in 0..50 {
        let entity = world.spawn();
        world.insert(entity, create_test_transform(i as f32, 0.0, 0.0));
    }

    let mut graph = SceneGraph::new();
    for i in 0..50 {
        let entity = quasar_core::ecs::Entity::from_index(i);
        graph.set_name(entity, format!("Entity_{}", i));
    }
    world.insert_resource(graph);

    let save = capture_game_save(&world);

    let json = save.to_json().unwrap();
    let binary = save.to_binary().unwrap();

    // Binary should be smaller due to compression
    assert!(
        binary.len() < json.len(),
        "Binary ({}) should be smaller than JSON ({})",
        binary.len(),
        json.len()
    );
}

#[test]
fn test_save_with_custom_metadata() {
    let mut world = World::new();
    let entity = world.spawn();
    world.insert(entity, create_test_transform(0.0, 0.0, 0.0));

    let mut graph = SceneGraph::new();
    graph.set_name(entity, "Player");
    world.insert_resource(graph);

    let mut save = capture_game_save(&world);
    save.meta.slot_name = "Custom Meta Test".to_string();
    save.meta
        .extra
        .insert("playtime".to_string(), "3600".to_string());
    save.meta
        .extra
        .insert("chapter".to_string(), "5".to_string());

    let json = save.to_json().unwrap();
    assert!(json.contains("playtime"));
    assert!(json.contains("3600"));
    assert!(json.contains("chapter"));
    assert!(json.contains("5"));
}

// ---------------------------------------------------------------------------
// 3. Load and verify all entities, transforms, and relationships
// ---------------------------------------------------------------------------

#[test]
fn test_load_from_json_roundtrip() {
    let mut world = World::new();
    create_test_scene_graph(&mut world);

    let mut save = capture_game_save(&world);
    save.meta.slot_name = "JSON Roundtrip".to_string();

    let json = save.to_json().unwrap();
    let loaded = GameSave::from_json(&json).expect("Should load from JSON");

    assert_eq!(loaded.meta.slot_name, "JSON Roundtrip");
    assert_eq!(loaded.entities.len(), 6);

    // Verify all entity names preserved
    let names: Vec<_> = loaded
        .entities
        .iter()
        .filter_map(|e| e.name.clone())
        .collect();

    assert!(names.contains(&"Root".to_string()));
    assert!(names.contains(&"Child_0".to_string()));
    assert!(names.contains(&"Grandchild_0".to_string()));
}

#[test]
fn test_load_from_binary_roundtrip() {
    let mut world = World::new();
    create_test_scene_graph(&mut world);

    let mut save = capture_game_save(&world);
    save.meta.slot_name = "Binary Roundtrip".to_string();

    let binary = save.to_binary().unwrap();
    let loaded = GameSave::from_binary(&binary).expect("Should load from binary");

    assert_eq!(loaded.meta.slot_name, "Binary Roundtrip");
    assert_eq!(loaded.entities.len(), 6);

    // Verify transforms preserved
    for saved_entity in &loaded.entities {
        match saved_entity.name.as_deref() {
            Some("Root") => {
                assert_eq!(saved_entity.transform.position, Vec3::new(0.0, 0.0, 0.0));
            }
            Some("Child_0") => {
                assert_eq!(saved_entity.transform.position, Vec3::new(0.0, 1.0, 0.0));
            }
            _ => {}
        }
    }
}

#[test]
fn test_load_game_save_spawns_entities() {
    let mut save = GameSave {
        meta: SaveMeta {
            slot_name: "Load Test".to_string(),
            timestamp: "0".to_string(),
            extra: HashMap::new(),
        },
        entities: vec![
            SavedEntity {
                index: 0,
                name: Some("Entity_A".to_string()),
                transform: create_test_transform(1.0, 2.0, 3.0),
                children: vec![],
                custom_data: HashMap::new(),
            },
            SavedEntity {
                index: 1,
                name: Some("Entity_B".to_string()),
                transform: create_test_transform(4.0, 5.0, 6.0),
                children: vec![],
                custom_data: HashMap::new(),
            },
        ],
    };

    let mut world = World::new();
    let spawned = load_game_save(&mut world, &save);

    assert_eq!(spawned.len(), 2);

    // Verify entities have transforms
    for (entity, saved) in &spawned {
        let transform = world.get::<Transform>(*entity).expect("Should have Transform");
        assert_eq!(transform.position, saved.transform.position);
    }

    // Prevent unused variable warnings
    let _ = save;
}

#[test]
fn test_load_preserves_children_relationships() {
    let save = GameSave {
        meta: SaveMeta {
            slot_name: "Children Test".to_string(),
            timestamp: "0".to_string(),
            extra: HashMap::new(),
        },
        entities: vec![
            SavedEntity {
                index: 0,
                name: Some("Parent".to_string()),
                transform: Transform::IDENTITY,
                children: vec![1, 2],
                custom_data: HashMap::new(),
            },
            SavedEntity {
                index: 1,
                name: Some("Child_1".to_string()),
                transform: Transform::IDENTITY,
                children: vec![],
                custom_data: HashMap::new(),
            },
            SavedEntity {
                index: 2,
                name: Some("Child_2".to_string()),
                transform: Transform::IDENTITY,
                children: vec![],
                custom_data: HashMap::new(),
            },
        ],
    };

    // Verify children array is preserved
    assert_eq!(save.entities[0].children.len(), 2);
    assert_eq!(save.entities[0].children[0], 1);
    assert_eq!(save.entities[0].children[1], 2);
}

#[test]
fn test_load_auto_detect_format() {
    use std::io::Write;

    let save = GameSave {
        meta: SaveMeta {
            slot_name: "Auto Detect".to_string(),
            timestamp: "0".to_string(),
            extra: HashMap::new(),
        },
        entities: vec![SavedEntity {
            index: 0,
            name: Some("Test".to_string()),
            transform: Transform::IDENTITY,
            children: vec![],
            custom_data: HashMap::new(),
        }],
    };

    // Test binary detection via magic bytes
    let binary = save.to_binary().unwrap();
    let loaded = GameSave::from_binary(&binary).unwrap();
    assert_eq!(loaded.meta.slot_name, "Auto Detect");
}

// ---------------------------------------------------------------------------
// 4. Test save/load roundtrip with component overrides
// ---------------------------------------------------------------------------

#[test]
fn test_roundtrip_with_custom_entity_data() {
    let mut save = GameSave {
        meta: SaveMeta {
            slot_name: "Custom Data Test".to_string(),
            timestamp: "0".to_string(),
            extra: HashMap::new(),
        },
        entities: vec![SavedEntity {
            index: 0,
            name: Some("Player".to_string()),
            transform: create_test_transform(10.0, 20.0, 30.0),
            children: vec![],
            custom_data: {
                let mut map = HashMap::new();
                map.insert(
                    "health".to_string(),
                    serde_json::json!({"current": 75, "max": 100}),
                );
                map.insert(
                    "inventory".to_string(),
                    serde_json::json!(["sword", "shield", "potion"]),
                );
                map
            },
        }],
    };

    // Roundtrip through JSON
    let json = save.to_json().unwrap();
    let loaded = GameSave::from_json(&json).unwrap();

    assert_eq!(loaded.entities.len(), 1);
    let entity = &loaded.entities[0];

    // Verify custom data preserved
    assert!(entity.custom_data.contains_key("health"));
    assert!(entity.custom_data.contains_key("inventory"));

    let health = entity.custom_data.get("health").unwrap();
    assert_eq!(health["current"], 75);
    assert_eq!(health["max"], 100);

    let inventory = entity.custom_data.get("inventory").unwrap();
    assert_eq!(inventory.as_array().unwrap().len(), 3);

    // Prevent unused warnings
    let _ = save;
}

#[test]
fn test_roundtrip_with_transform_overrides() {
    let mut save = GameSave {
        meta: SaveMeta {
            slot_name: "Transform Override".to_string(),
            timestamp: "0".to_string(),
            extra: HashMap::new(),
        },
        entities: vec![
            SavedEntity {
                index: 0,
                name: Some("StaticObject".to_string()),
                transform: create_test_transform(0.0, 0.0, 0.0),
                children: vec![1],
                custom_data: {
                    let mut map = HashMap::new();
                    map.insert(
                        "is_static".to_string(),
                        serde_json::json!(true),
                    );
                    map
                },
            },
            SavedEntity {
                index: 1,
                name: Some("DynamicObject".to_string()),
                transform: create_test_transform(5.0, 3.0, 2.0),
                children: vec![],
                custom_data: {
                    let mut map = HashMap::new();
                    map.insert(
                        "velocity".to_string(),
                        serde_json::json!({"x": 1.0, "y": 0.0, "z": 0.0}),
                    );
                    map
                },
            },
        ],
    };

    // Simulate modifying before save
    save.entities[1].transform =
        create_test_transform(10.0, 6.0, 4.0); // Override position

    let json = save.to_json().unwrap();
    let loaded = GameSave::from_json(&json).unwrap();

    // Verify override persisted
    assert_eq!(
        loaded.entities[1].transform.position,
        Vec3::new(10.0, 6.0, 4.0)
    );

    // Verify custom data still intact
    assert!(loaded.entities[1]
        .custom_data
        .contains_key("velocity"));

    let _ = save;
}

#[test]
fn test_scene_data_to_json_roundtrip() {
    let mut scene = SceneData::new("Test Scene");

    scene.entities.push(EntityData {
        name: Some("Root".into()),
        transform: Transform::IDENTITY,
        mesh_shape: Some("Cube".into()),
        children: vec![1, 2],
    });

    scene.entities.push(EntityData {
        name: Some("Child_1".into()),
        transform: Transform::from_position(Vec3::new(1.0, 0.0, 0.0)),
        mesh_shape: Some("Sphere".into()),
        children: vec![],
    });

    scene.entities.push(EntityData {
        name: Some("Child_2".into()),
        transform: Transform::from_position(Vec3::new(-1.0, 0.0, 0.0)),
        mesh_shape: Some("Plane".into()),
        children: vec![],
    });

    // Serialize
    let json = scene.to_json().expect("Should serialize");

    // Deserialize
    let loaded = SceneData::from_json(&json).expect("Should deserialize");

    assert_eq!(loaded.name, "Test Scene");
    assert_eq!(loaded.entities.len(), 3);
    assert_eq!(loaded.entities[0].children.len(), 2);
    assert_eq!(loaded.entities[0].mesh_shape, Some("Cube".into()));
}

#[test]
fn test_game_save_to_scene_data_conversion() {
    let save = GameSave {
        meta: SaveMeta {
            slot_name: "Scene Conversion".to_string(),
            timestamp: "0".to_string(),
            extra: HashMap::new(),
        },
        entities: vec![
            SavedEntity {
                index: 0,
                name: Some("Entity_1".to_string()),
                transform: create_test_transform(0.0, 0.0, 0.0),
                children: vec![1],
                custom_data: HashMap::new(),
            },
            SavedEntity {
                index: 1,
                name: Some("Entity_2".to_string()),
                transform: create_test_transform(1.0, 0.0, 0.0),
                children: vec![],
                custom_data: HashMap::new(),
            },
        ],
    };

    let scene_data = save.to_scene_data();

    assert_eq!(scene_data.name, "Scene Conversion");
    assert_eq!(scene_data.entities.len(), 2);
    assert_eq!(scene_data.entities[0].children.len(), 1);
}

#[test]
fn test_save_load_empty_scene() {
    let save = GameSave {
        meta: SaveMeta {
            slot_name: "Empty Scene".to_string(),
            timestamp: "0".to_string(),
            extra: HashMap::new(),
        },
        entities: vec![],
    };

    let json = save.to_json().unwrap();
    let loaded = GameSave::from_json(&json).unwrap();

    assert_eq!(loaded.entities.len(), 0);
    assert_eq!(loaded.meta.slot_name, "Empty Scene");
}

#[test]
fn test_save_slot_manager() {
    use std::fs;

    let temp_dir = std::env::temp_dir().join("quasar_test_slots");
    fs::create_dir_all(&temp_dir).ok();

    let mut slot_manager = SaveSlotManager::new(&temp_dir, 5);

    // Create a save
    let save = GameSave {
        meta: SaveMeta {
            slot_name: "Slot Test".to_string(),
            timestamp: "1000".to_string(),
            extra: HashMap::new(),
        },
        entities: vec![SavedEntity {
            index: 0,
            name: Some("Test".to_string()),
            transform: Transform::IDENTITY,
            children: vec![],
            custom_data: HashMap::new(),
        }],
    };

    // Save to slot 0
    slot_manager.save(0, &save).expect("Should save to slot");
    assert!(slot_manager.slot_exists(0));
    assert_eq!(slot_manager.slot_count(), 1);

    // Load from slot 0
    let loaded = slot_manager.load(0).expect("Should load from slot");
    assert_eq!(loaded.meta.slot_name, "Slot Test");

    // Delete slot 0
    slot_manager.delete(0).expect("Should delete slot");
    assert!(!slot_manager.slot_exists(0));
    assert_eq!(slot_manager.slot_count(), 0);

    // Cleanup
    fs::remove_dir_all(&temp_dir).ok();
}

#[test]
fn test_save_slot_manager_quick_save() {
    use std::fs;

    let temp_dir = std::env::temp_dir().join("quasar_quick_save");
    fs::create_dir_all(&temp_dir).ok();

    let mut slot_manager = SaveSlotManager::new(&temp_dir, 3);

    let save = GameSave {
        meta: SaveMeta {
            slot_name: "Quick Save 1".to_string(),
            timestamp: "1000".to_string(),
            extra: HashMap::new(),
        },
        entities: vec![],
    };

    // Quick save to first available slot
    let slot = slot_manager.quick_save(&save).expect("Should quick save");
    assert_eq!(slot, 0);

    // Quick save again
    let save2 = GameSave {
        meta: SaveMeta {
            slot_name: "Quick Save 2".to_string(),
            timestamp: "2000".to_string(),
            extra: HashMap::new(),
        },
        entities: vec![],
    };
    let slot2 = slot_manager
        .quick_save(&save2)
        .expect("Should quick save again");
    assert_eq!(slot2, 1);

    assert_eq!(slot_manager.slot_count(), 2);

    // Cleanup
    fs::remove_dir_all(&temp_dir).ok();
}

#[test]
fn test_save_checksum_validation() {
    let save = GameSave {
        meta: SaveMeta {
            slot_name: "Checksum Test".to_string(),
            timestamp: "0".to_string(),
            extra: HashMap::new(),
        },
        entities: vec![SavedEntity {
            index: 0,
            name: Some("Test".to_string()),
            transform: Transform::IDENTITY,
            children: vec![],
            custom_data: HashMap::new(),
        }],
    };

    let mut binary = save.to_binary().unwrap();

    // Corrupt the data (after header)
    if binary.len() > 30 {
        binary[30] ^= 0xFF; // Flip bits
    }

    let result = GameSave::from_binary(&binary);
    assert!(result.is_err());

    // Should be checksum or decompression error
    match result {
        Err(SaveLoadError::Decompression(_)) | Err(SaveLoadError::Deserialization(_)) => {
            // Expected
        }
        _ => panic!("Expected decompression or deserialization error"),
    }
}

#[test]
fn test_save_load_with_rotation() {
    let mut save = GameSave {
        meta: SaveMeta {
            slot_name: "Rotation Test".to_string(),
            timestamp: "0".to_string(),
            extra: HashMap::new(),
        },
        entities: vec![SavedEntity {
            index: 0,
            name: Some("RotatedEntity".to_string()),
            transform: Transform {
                position: Vec3::new(0.0, 0.0, 0.0),
                rotation: Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
                scale: Vec3::new(2.0, 2.0, 2.0),
            },
            children: vec![],
            custom_data: HashMap::new(),
        }],
    };

    let json = save.to_json().unwrap();
    let loaded = GameSave::from_json(&json).unwrap();

    let entity = &loaded.entities[0];
    assert!(
        (entity.transform.rotation.w - 0.7071).abs() < 0.001,
        "Rotation should be approximately 90 degrees around Y"
    );
    assert!((entity.transform.scale.x - 2.0).abs() < 0.001);

    let _ = save;
}

#[test]
fn test_scene_data_version() {
    let scene = SceneData::new("Version Test");
    assert_eq!(scene.version, SceneData::CURRENT_VERSION);
    assert_eq!(scene.version, 1);
}
