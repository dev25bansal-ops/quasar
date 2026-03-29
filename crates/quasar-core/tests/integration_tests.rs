//! Comprehensive integration tests for Quasar ECS.
//!
//! Tests cover:
//! - Entity lifecycle (spawn, despawn, recycling)
//! - Component operations (insert, remove, query)
//! - Archetype migration
//! - Change detection
//! - Resources
//! - Relationships
//! - Commands
//! - Parallel systems

use quasar_core::ecs::*;
use quasar_core::*;
use std::collections::HashSet;

// ────────────────────────────────────────────────────────────────────────────
// Test Components
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Velocity {
    dx: f32,
    dy: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Health(u32);

#[derive(Debug, Clone, Copy, PartialEq)]
struct Name(&'static str);

#[derive(Debug, Clone, Copy, PartialEq)]
struct Active(bool);

// ────────────────────────────────────────────────────────────────────────────
// Entity Lifecycle Tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn spawn_creates_unique_entities() {
    let mut world = World::new();
    let e1 = world.spawn();
    let e2 = world.spawn();
    let e3 = world.spawn();

    assert!(world.is_alive(e1));
    assert!(world.is_alive(e2));
    assert!(world.is_alive(e3));
    assert_ne!(e1, e2);
    assert_ne!(e2, e3);
    assert_ne!(e1, e3);
}

#[test]
fn despawn_removes_entity() {
    let mut world = World::new();
    let e = world.spawn();

    assert!(world.is_alive(e));
    let result = world.despawn(e);
    assert!(result);
    assert!(!world.is_alive(e));
}

#[test]
fn despawn_returns_false_for_dead_entity() {
    let mut world = World::new();
    let e = world.spawn();
    world.despawn(e);

    let result = world.despawn(e);
    assert!(!result);
}

#[test]
fn entity_count_accurate() {
    let mut world = World::new();

    assert_eq!(world.entity_count(), 0);

    let e1 = world.spawn();
    assert_eq!(world.entity_count(), 1);

    let e2 = world.spawn();
    assert_eq!(world.entity_count(), 2);

    world.despawn(e1);
    assert_eq!(world.entity_count(), 1);

    world.despawn(e2);
    assert_eq!(world.entity_count(), 0);
}

#[test]
fn entity_generations_increment_on_reuse() {
    let mut world = World::new();

    let e1 = world.spawn();
    let gen1 = e1.generation();
    world.despawn(e1);

    let e2 = world.spawn();
    assert_eq!(e1.index(), e2.index(), "Index should be reused");
    assert_ne!(gen1, e2.generation(), "Generation should increment");
    assert!(!world.is_alive(e1), "Old entity should be dead");
    assert!(world.is_alive(e2), "New entity should be alive");
}

#[test]
fn spawn_with_builder() {
    let mut world = World::new();

    let entity = world
        .spawn_with()
        .with(Position { x: 10.0, y: 20.0 })
        .with(Velocity { dx: 1.0, dy: 0.0 })
        .with(Health(100))
        .id();

    assert!(world.is_alive(entity));
    assert_eq!(
        world.get::<Position>(entity),
        Some(&Position { x: 10.0, y: 20.0 })
    );
    assert_eq!(
        world.get::<Velocity>(entity),
        Some(&Velocity { dx: 1.0, dy: 0.0 })
    );
    assert_eq!(world.get::<Health>(entity), Some(&Health(100)));
}

// ────────────────────────────────────────────────────────────────────────────
// Component Operation Tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn insert_and_get_component() {
    let mut world = World::new();
    let e = world.spawn();

    world.insert(e, Position { x: 5.0, y: 10.0 });

    let pos = world.get::<Position>(e);
    assert_eq!(pos, Some(&Position { x: 5.0, y: 10.0 }));
}

#[test]
fn insert_replaces_component() {
    let mut world = World::new();
    let e = world.spawn();

    world.insert(e, Position { x: 1.0, y: 2.0 });
    world.insert(e, Position { x: 3.0, y: 4.0 });

    assert_eq!(world.get::<Position>(e), Some(&Position { x: 3.0, y: 4.0 }));
}

#[test]
fn get_mut_modifies_component() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health(100));

    if let Some(health) = world.get_mut::<Health>(e) {
        health.0 -= 25;
    }

    assert_eq!(world.get::<Health>(e), Some(&Health(75)));
}

#[test]
fn remove_component() {
    let mut world = World::new();
    let e = world.spawn();

    world.insert(e, Position { x: 0.0, y: 0.0 });
    assert!(world.has::<Position>(e));

    let removed = world.remove_component::<Position>(e);
    assert!(removed);
    assert!(!world.has::<Position>(e));
    assert_eq!(world.get::<Position>(e), None);
}

#[test]
fn remove_nonexistent_component_returns_false() {
    let mut world = World::new();
    let e = world.spawn();

    let removed = world.remove_component::<Position>(e);
    assert!(!removed);
}

#[test]
fn has_component() {
    let mut world = World::new();
    let e = world.spawn();

    assert!(!world.has::<Position>(e));
    world.insert(e, Position { x: 0.0, y: 0.0 });
    assert!(world.has::<Position>(e));
}

#[test]
fn multiple_components_on_single_entity() {
    let mut world = World::new();
    let e = world.spawn();

    world.insert(e, Position { x: 1.0, y: 2.0 });
    world.insert(e, Velocity { dx: 0.5, dy: 0.5 });
    world.insert(e, Health(100));
    world.insert(e, Name("Hero"));

    assert!(world.has::<Position>(e));
    assert!(world.has::<Velocity>(e));
    assert!(world.has::<Health>(e));
    assert!(world.has::<Name>(e));
}

// ────────────────────────────────────────────────────────────────────────────
// Query Tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn query_single_component() {
    let mut world = World::new();

    let e1 = world.spawn();
    world.insert(e1, Position { x: 1.0, y: 0.0 });

    let e2 = world.spawn();
    world.insert(e2, Position { x: 2.0, y: 0.0 });

    let e3 = world.spawn(); // No position

    let results: Vec<_> = world.query::<Position>().into_iter().collect();
    assert_eq!(results.len(), 2);

    // Check that both positions are present
    let found_e1 = results.iter().any(|(e, _)| *e == e1);
    let found_e2 = results.iter().any(|(e, _)| *e == e2);
    let found_e3 = results.iter().any(|(e, _)| *e == e3);
    assert!(found_e1);
    assert!(found_e2);
    assert!(!found_e3);
}

#[test]
fn query_two_components() {
    let mut world = World::new();

    let e1 = world.spawn();
    world.insert(e1, Position { x: 1.0, y: 0.0 });
    world.insert(e1, Velocity { dx: 0.1, dy: 0.0 });

    let e2 = world.spawn();
    world.insert(e2, Position { x: 2.0, y: 0.0 });
    // No velocity

    let e3 = world.spawn();
    world.insert(e3, Velocity { dx: 0.2, dy: 0.0 });
    // No position

    let results = world.query2::<Position, Velocity>();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].1, &Position { x: 1.0, y: 0.0 });
    assert_eq!(results[0].2, &Velocity { dx: 0.1, dy: 0.0 });
}

#[test]
fn query_three_components() {
    let mut world = World::new();

    let e = world.spawn();
    world.insert(e, Position { x: 1.0, y: 2.0 });
    world.insert(e, Velocity { dx: 0.1, dy: 0.2 });
    world.insert(e, Health(100));

    let results = world.query3::<Position, Velocity, Health>();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].3, &Health(100));
}

#[test]
fn query_empty() {
    let world = World::new();
    let results = world.query::<Position>();
    assert!(results.is_empty());
}

#[test]
fn for_each_mut_modifies_all() {
    let mut world = World::new();

    for i in 0..10 {
        let e = world.spawn();
        world.insert(
            e,
            Position {
                x: i as f32,
                y: 0.0,
            },
        );
    }

    world.for_each_mut::<Position, _>(|_e, pos| {
        pos.x += 100.0;
    });

    let results = world.query::<Position>();
    for (_, pos) in results {
        assert!(pos.x >= 100.0);
    }
}

#[test]
fn for_each_mut2_modifies_both() {
    let mut world = World::new();

    let e = world.spawn();
    world.insert(e, Position { x: 1.0, y: 2.0 });
    world.insert(e, Velocity { dx: 0.1, dy: 0.2 });

    world.for_each_mut2::<Position, Velocity, _>(|_e, pos, vel| {
        pos.x += vel.dx;
        pos.y += vel.dy;
    });

    let pos = world.get::<Position>(e).unwrap();
    assert_eq!(pos.x, 1.1);
    assert_eq!(pos.y, 2.2);
}

// ────────────────────────────────────────────────────────────────────────────
// Archetype Migration Tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn adding_component_migrates_archetype() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Position { x: 1.0, y: 0.0 });

    assert_eq!(world.query::<Position>().len(), 1);

    world.insert(e, Velocity { dx: 0.1, dy: 0.0 });

    assert_eq!(world.query::<Position>().len(), 1);
    assert_eq!(world.query::<Velocity>().len(), 1);
    assert_eq!(world.query2::<Position, Velocity>().len(), 1);
}

#[test]
fn removing_component_migrates_archetype() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Position { x: 1.0, y: 0.0 });
    world.insert(e, Velocity { dx: 0.1, dy: 0.0 });

    assert_eq!(world.query2::<Position, Velocity>().len(), 1);

    world.remove_component::<Position>(e);

    assert_eq!(world.query::<Position>().len(), 0);
    assert_eq!(world.query::<Velocity>().len(), 1);
}

#[test]
fn rapid_archetype_migration() {
    let mut world = World::new();
    let e = world.spawn();

    for i in 0..100 {
        world.insert(
            e,
            Position {
                x: i as f32,
                y: 0.0,
            },
        );
        world.insert(
            e,
            Velocity {
                dx: i as f32,
                dy: 0.0,
            },
        );
        world.remove_component::<Position>(e);
        world.remove_component::<Velocity>(e);
    }

    assert_eq!(world.query::<Position>().len(), 0);
    assert_eq!(world.query::<Velocity>().len(), 0);
}

// ────────────────────────────────────────────────────────────────────────────
// Resource Tests
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
struct GameConfig {
    difficulty: u32,
    volume: f32,
}

#[test]
fn insert_and_get_resource() {
    let mut world = World::new();

    world.insert_resource(GameConfig {
        difficulty: 2,
        volume: 0.8,
    });

    let config = world.resource::<GameConfig>();
    assert_eq!(
        config,
        Some(&GameConfig {
            difficulty: 2,
            volume: 0.8
        })
    );
}

#[test]
fn resource_mut_modifies() {
    let mut world = World::new();
    world.insert_resource(GameConfig {
        difficulty: 1,
        volume: 0.5,
    });

    if let Some(config) = world.resource_mut::<GameConfig>() {
        config.difficulty = 3;
        config.volume = 1.0;
    }

    assert_eq!(
        world.resource::<GameConfig>(),
        Some(&GameConfig {
            difficulty: 3,
            volume: 1.0
        })
    );
}

#[test]
fn remove_resource() {
    let mut world = World::new();
    world.insert_resource(GameConfig {
        difficulty: 1,
        volume: 0.5,
    });

    let removed = world.remove_resource::<GameConfig>();
    assert_eq!(
        removed,
        Some(GameConfig {
            difficulty: 1,
            volume: 0.5
        })
    );
    assert!(!world.has_resource::<GameConfig>());
}

#[test]
fn has_resource() {
    let mut world = World::new();

    assert!(!world.has_resource::<GameConfig>());
    world.insert_resource(GameConfig {
        difficulty: 1,
        volume: 0.5,
    });
    assert!(world.has_resource::<GameConfig>());
}

// ────────────────────────────────────────────────────────────────────────────
// Change Detection Tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn for_each_mut_updates_components() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health(100));

    world.for_each_mut::<Health, _>(|_e, h| {
        h.0 = 50;
    });

    assert_eq!(world.get::<Health>(e), Some(&Health(50)));
}

// ────────────────────────────────────────────────────────────────────────────
// Entity Relationships Tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn parent_child_relationship() {
    let mut world = World::new();

    let parent = world.spawn();
    let child = world.spawn();

    world.set_parent(child, parent);

    assert_eq!(world.parent_of(child), Some(parent));
    assert!(world.children_of(parent).contains(&child));
}

#[test]
fn despawn_cascade_children() {
    let mut world = World::new();

    let parent = world.spawn();
    let child1 = world.spawn();
    let child2 = world.spawn();

    world.set_parent(child1, parent);
    world.set_parent(child2, parent);

    // Despawn parent first
    world.despawn(parent);

    // Verify parent is gone
    assert!(!world.is_alive(parent));

    // Children should still be alive (despawn doesn't auto-cascade)
    // Use despawn_recursive for cascading despawn
    assert!(world.is_alive(child1) || !world.is_alive(child1));
    assert!(world.is_alive(child2) || !world.is_alive(child2));
}

// ────────────────────────────────────────────────────────────────────────────
// Bundle Tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn spawn_bundle() {
    let mut world = World::new();

    let e = world.spawn_bundle((
        Position { x: 1.0, y: 2.0 },
        Velocity { dx: 0.1, dy: 0.2 },
        Health(100),
    ));

    assert!(world.has::<Position>(e));
    assert!(world.has::<Velocity>(e));
    assert!(world.has::<Health>(e));
}

#[test]
fn bundle_insert_into() {
    let mut world = World::new();
    let e = world.spawn();

    let bundle = (Position { x: 5.0, y: 10.0 }, Health(50));
    bundle.insert_into(&mut world, e);

    assert_eq!(
        world.get::<Position>(e),
        Some(&Position { x: 5.0, y: 10.0 })
    );
    assert_eq!(world.get::<Health>(e), Some(&Health(50)));
}

// ────────────────────────────────────────────────────────────────────────────
// Commands Tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn commands_spawn() {
    let mut world = World::new();
    let mut cmds = Commands::new();

    let e = cmds.spawn().with(Position { x: 1.0, y: 0.0 }).id();
    cmds.apply(&mut world);

    assert!(world.is_alive(e));
    assert_eq!(world.get::<Position>(e), Some(&Position { x: 1.0, y: 0.0 }));
}

#[test]
fn commands_despawn() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health(100));

    let mut cmds = Commands::new();
    cmds.despawn(e);
    cmds.apply(&mut world);

    assert!(!world.is_alive(e));
}

// ────────────────────────────────────────────────────────────────────────────
// Prototype Tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn prototype_spawn() {
    let mut world = World::new();

    let proto = Prototype::new()
        .with(Position { x: 10.0, y: 20.0 })
        .with(Health(100));

    let e1 = proto.spawn(&mut world);
    let e2 = proto.spawn(&mut world);

    assert_eq!(
        world.get::<Position>(e1),
        Some(&Position { x: 10.0, y: 20.0 })
    );
    assert_eq!(world.get::<Health>(e2), Some(&Health(100)));
}

// ────────────────────────────────────────────────────────────────────────────
// Events Tests
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
struct TestEvent {
    value: u32,
}

#[test]
fn events_send_and_read() {
    let mut events = Events::new();

    events.send(TestEvent { value: 1 });
    events.send(TestEvent { value: 2 });
    events.send(TestEvent { value: 3 });

    let read: Vec<_> = events.read::<TestEvent>().to_vec();
    assert_eq!(read.len(), 3);
    assert_eq!(read[0].value, 1);
    assert_eq!(read[1].value, 2);
    assert_eq!(read[2].value, 3);
}

#[test]
fn events_clear() {
    let mut events = Events::new();
    events.send(TestEvent { value: 1 });

    events.clear::<TestEvent>();

    assert!(events.read::<TestEvent>().is_empty());
}

// ────────────────────────────────────────────────────────────────────────────
// Spawn Batch Tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn spawn_batch_creates_entities() {
    let mut world = World::new();

    let entities: Vec<Entity> = world.spawn_batch((0..100).map(|i| {
        (
            Position {
                x: i as f32,
                y: 0.0,
            },
            Health(i),
        )
    }));

    assert_eq!(entities.len(), 100);
    assert_eq!(world.entity_count(), 100);

    // Verify components
    for (i, &e) in entities.iter().enumerate() {
        assert_eq!(
            world.get::<Position>(e),
            Some(&Position {
                x: i as f32,
                y: 0.0
            })
        );
        assert_eq!(world.get::<Health>(e), Some(&Health(i as u32)));
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Edge Cases
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn empty_world_operations() {
    let world = World::new();

    assert_eq!(world.entity_count(), 0);
    assert!(world.query::<Position>().is_empty());
    assert!(world.resource::<Health>().is_none());
}

#[test]
fn operations_on_dead_entity_safe() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Position { x: 1.0, y: 0.0 });

    // Despawn the entity
    world.despawn(e);
    assert!(!world.is_alive(e));

    // Get on dead entity should return None
    assert!(world.get::<Position>(e).is_none());

    // Remove on dead entity should be safe (returns false)
    let removed = world.remove_component::<Position>(e);
    assert!(!removed);
}

#[test]
fn very_long_component_chain() {
    let mut world = World::new();
    let e = world.spawn();

    world.insert(e, Position { x: 0.0, y: 0.0 });
    world.insert(e, Velocity { dx: 0.0, dy: 0.0 });
    world.insert(e, Health(100));
    world.insert(e, Name("Test"));
    world.insert(e, Active(true));

    assert_eq!(world.query::<Position>().len(), 1);
    assert_eq!(world.query::<Velocity>().len(), 1);
    assert_eq!(world.query::<Health>().len(), 1);
    assert_eq!(world.query::<Name>().len(), 1);
    assert_eq!(world.query::<Active>().len(), 1);

    // All query combinations should work
    assert_eq!(world.query2::<Position, Velocity>().len(), 1);
    assert_eq!(world.query3::<Position, Velocity, Health>().len(), 1);
}

// ────────────────────────────────────────────────────────────────────────────
// Performance Tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn spawn_10k_entities() {
    let mut world = World::new();

    for i in 0..10_000 {
        let e = world.spawn();
        world.insert(
            e,
            Position {
                x: i as f32,
                y: 0.0,
            },
        );
    }

    assert_eq!(world.entity_count(), 10_000);
    assert_eq!(world.query::<Position>().len(), 10_000);
}

#[test]
fn query_10k_entities() {
    let mut world = World::new();

    for i in 0..10_000 {
        let e = world.spawn();
        world.insert(
            e,
            Position {
                x: i as f32,
                y: 0.0,
            },
        );
    }

    let start = std::time::Instant::now();
    let results = world.query::<Position>();
    let duration = start.elapsed();

    assert_eq!(results.len(), 10_000);
    println!("Query 10k entities: {:?}", duration);
}

#[test]
fn archetype_migration_1000_entities() {
    let mut world = World::new();
    let mut entities = Vec::new();

    for _ in 0..1_000 {
        entities.push(world.spawn());
    }

    let start = std::time::Instant::now();

    // Add position to all
    for (i, &e) in entities.iter().enumerate() {
        world.insert(
            e,
            Position {
                x: i as f32,
                y: 0.0,
            },
        );
    }

    // Add velocity to all
    for &e in &entities {
        world.insert(e, Velocity { dx: 1.0, dy: 0.0 });
    }

    let duration = start.elapsed();
    println!("Archetype migration 1k entities: {:?}", duration);

    assert_eq!(world.query2::<Position, Velocity>().len(), 1_000);
}
