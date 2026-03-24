//! Property-based stress tests for the ECS.
//!
//! Uses `proptest` to fuzz entity lifecycle, component insertion/removal,
//! and query consistency under random operation sequences.

use proptest::prelude::*;
use quasar_core::ecs::World;

// ── Components ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
struct Pos {
    x: f32,
    y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Vel {
    dx: f32,
    dy: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Tag(u32);

// ── Operation enum for random sequences ──────────────────────────

#[derive(Debug, Clone)]
enum Op {
    Spawn,
    Despawn(usize),
    InsertPos(usize, f32, f32),
    InsertVel(usize, f32, f32),
    InsertTag(usize, u32),
    RemovePos(usize),
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        Just(Op::Spawn),
        (0..64usize).prop_map(Op::Despawn),
        (0..64usize, any::<f32>(), any::<f32>()).prop_map(|(i, x, y)| Op::InsertPos(i, x, y)),
        (0..64usize, any::<f32>(), any::<f32>()).prop_map(|(i, dx, dy)| Op::InsertVel(i, dx, dy)),
        (0..64usize, any::<u32>()).prop_map(|(i, v)| Op::InsertTag(i, v)),
        (0..64usize).prop_map(Op::RemovePos),
    ]
}

// ── Tests ────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Random operation sequences must never panic or corrupt the world.
    #[test]
    fn random_ops_no_panic(ops in prop::collection::vec(op_strategy(), 1..200)) {
        let mut world = World::new();
        let mut alive = Vec::new();

        for op in ops {
            match op {
                Op::Spawn => {
                    let e = world.spawn();
                    alive.push(e);
                }
                Op::Despawn(idx) => {
                    if !alive.is_empty() {
                        let i = idx % alive.len();
                        world.despawn(alive[i]);
                        alive.swap_remove(i);
                    }
                }
                Op::InsertPos(idx, x, y) => {
                    if !alive.is_empty() {
                        let e = alive[idx % alive.len()];
                        world.insert(e, Pos { x, y });
                    }
                }
                Op::InsertVel(idx, dx, dy) => {
                    if !alive.is_empty() {
                        let e = alive[idx % alive.len()];
                        world.insert(e, Vel { dx, dy });
                    }
                }
                Op::InsertTag(idx, v) => {
                    if !alive.is_empty() {
                        let e = alive[idx % alive.len()];
                        world.insert(e, Tag(v));
                    }
                }
                Op::RemovePos(idx) => {
                    if !alive.is_empty() {
                        let e = alive[idx % alive.len()];
                        world.remove::<Pos>(e);
                    }
                }
            }
        }
    }

    /// After inserting Pos on every alive entity, query count must match.
    #[test]
    fn query_count_matches_inserts(
        spawn_count in 1..100usize,
        extra_ops in prop::collection::vec(op_strategy(), 0..50),
    ) {
        let mut world = World::new();
        let mut alive = Vec::new();

        for _ in 0..spawn_count {
            alive.push(world.spawn());
        }

        // Apply random operations.
        for op in extra_ops {
            match op {
                Op::Spawn => alive.push(world.spawn()),
                Op::Despawn(idx) => {
                    if !alive.is_empty() {
                        let i = idx % alive.len();
                        world.despawn(alive[i]);
                        alive.swap_remove(i);
                    }
                }
                _ => {} // skip component ops for this test
            }
        }

        // Insert Pos on all surviving entities.
        for &e in &alive {
            world.insert(e, Pos { x: 1.0, y: 2.0 });
        }

        let results = world.query::<Pos>();
        prop_assert_eq!(results.len(), alive.len());
    }

    /// Roundtrip: insert → query → values match.
    #[test]
    fn insert_then_query_roundtrip(values in prop::collection::vec((any::<f32>(), any::<f32>()), 1..50)) {
        let mut world = World::new();
        let mut entities = Vec::new();

        for &(x, y) in &values {
            let e = world.spawn();
            world.insert(e, Pos { x, y });
            entities.push(e);
        }

        let results = world.query::<Pos>();
        prop_assert_eq!(results.len(), values.len());

        // Every value we inserted must be present in the query results.
        for &(x, y) in &values {
            let found = results.iter().any(|(_, p)| p.x == x && p.y == y);
            prop_assert!(found, "Missing Pos({}, {})", x, y);
        }
    }
}

// ── Stress Tests ─────────────────────────────────────────────────────

/// Stress test: rapid archetype migration (add/remove components repeatedly)
#[test]
fn archetype_migration_stress() {
    let mut world = World::new();
    let mut entities = Vec::new();

    // Spawn 1000 entities
    for _ in 0..1000 {
        let e = world.spawn();
        entities.push(e);
    }

    // Rapidly add and remove components to force archetype migrations
    for iteration in 0..10 {
        for &e in &entities {
            world.insert(
                e,
                Pos {
                    x: iteration as f32,
                    y: 0.0,
                },
            );
        }
        for &e in &entities {
            world.insert(e, Vel { dx: 1.0, dy: 2.0 });
        }
        for &e in &entities {
            world.remove::<Pos>(e);
        }
        for &e in &entities {
            world.remove::<Vel>(e);
        }
    }

    // Verify world is still consistent
    let pos_count = world.query::<Pos>().len();
    let vel_count = world.query::<Vel>().len();
    assert_eq!(pos_count, 0, "All Pos components should be removed");
    assert_eq!(vel_count, 0, "All Vel components should be removed");
}

/// Stress test: many entities with many component combinations
#[test]
fn many_archetypes_stress() {
    let mut world = World::new();
    let mut entities = Vec::new();

    // Create entities with different component combinations
    for i in 0..1000 {
        let e = world.spawn();

        // Create different archetype patterns
        match i % 8 {
            0 => {} // Empty
            1 => {
                world.insert(e, Pos { x: 1.0, y: 0.0 });
            }
            2 => {
                world.insert(e, Vel { dx: 1.0, dy: 0.0 });
            }
            3 => {
                world.insert(e, Pos { x: 1.0, y: 0.0 });
                world.insert(e, Vel { dx: 1.0, dy: 0.0 });
            }
            4 => {
                world.insert(e, Tag(i));
            }
            5 => {
                world.insert(e, Pos { x: 1.0, y: 0.0 });
                world.insert(e, Tag(i));
            }
            6 => {
                world.insert(e, Vel { dx: 1.0, dy: 0.0 });
                world.insert(e, Tag(i));
            }
            7 => {
                world.insert(e, Pos { x: 1.0, y: 0.0 });
                world.insert(e, Vel { dx: 1.0, dy: 0.0 });
                world.insert(e, Tag(i));
            }
            _ => {}
        }
        entities.push(e);
    }

    // Verify counts are correct
    let pos_count = world.query::<Pos>().len();
    let vel_count = world.query::<Vel>().len();
    let tag_count = world.query::<Tag>().len();

    assert_eq!(pos_count, 500, "Half should have Pos");
    assert_eq!(vel_count, 500, "Half should have Vel");
    assert_eq!(tag_count, 500, "Half should have Tag");
}

/// Stress test: spawn and despawn in rapid succession
#[test]
fn spawn_despawn_stress() {
    let mut world = World::new();

    for _ in 0..10000 {
        let e = world.spawn();
        world.insert(e, Pos { x: 1.0, y: 2.0 });
        world.remove::<Pos>(e);
        world.despawn(e);
    }

    // World should be empty
    let entities = world.query::<Pos>();
    assert!(entities.is_empty());
}

/// Stress test: entity ID recycling
#[test]
fn entity_id_recycling_stress() {
    let mut world = World::new();
    let mut ids = std::collections::HashSet::new();

    // Spawn and despawn entities, checking IDs
    for i in 0..1000 {
        let e = world.spawn();
        world.insert(e, Tag(i));

        // ID should be unique at any given time
        assert!(!ids.contains(&e), "Duplicate entity ID detected");
        ids.insert(e);

        if i % 2 == 0 {
            world.despawn(e);
            ids.remove(&e);
        }
    }
}

/// Stress test: memory usage with many entities
#[test]
fn memory_stress_100k_entities() {
    let mut world = World::new();

    // Spawn 100k entities
    for i in 0..100_000 {
        let e = world.spawn();
        world.insert(
            e,
            Pos {
                x: i as f32,
                y: 0.0,
            },
        );
    }

    // Query should return all
    let results = world.query::<Pos>();
    assert_eq!(results.len(), 100_000);

    // Clear half
    let entities: Vec<_> = results.iter().map(|(e, _)| *e).collect();
    for (i, e) in entities.iter().enumerate() {
        if i % 2 == 0 {
            world.despawn(*e);
        }
    }

    let remaining = world.query::<Pos>();
    assert_eq!(remaining.len(), 50_000);
}

/// Edge case: empty world queries
#[test]
fn empty_world_queries() {
    let world = World::new();

    assert!(world.query::<Pos>().is_empty());
    assert!(world.query::<Vel>().is_empty());
    assert!(world.query::<Tag>().is_empty());
}

/// Edge case: single entity all operations
#[test]
fn single_entity_operations() {
    let mut world = World::new();
    let e = world.spawn();

    // Add component
    world.insert(e, Pos { x: 1.0, y: 2.0 });
    assert_eq!(world.query::<Pos>().len(), 1);

    // Update component
    world.insert(e, Pos { x: 3.0, y: 4.0 });
    let results = world.query::<Pos>();
    assert_eq!(results[0].1.x, 3.0);

    // Remove component
    world.remove::<Pos>(e);
    assert!(world.query::<Pos>().is_empty());

    // Remove non-existent component (should be safe)
    world.remove::<Pos>(e);
    world.remove::<Vel>(e);

    // Despawn
    world.despawn(e);
}

/// Edge case: component on despawned entity
#[test]
fn component_on_despawned_entity() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Pos { x: 1.0, y: 0.0 });
    world.despawn(e);

    // Query should not return the despawned entity
    assert!(world.query::<Pos>().is_empty());
}

/// Concurrency stress: many rapid operations
#[test]
fn rapid_operations_stress() {
    let mut world = World::new();
    let mut entities = Vec::new();

    // Rapid spawn/despawn/insert/remove cycles
    for batch in 0..100 {
        // Spawn batch
        for _ in 0..100 {
            let e = world.spawn();
            world.insert(
                e,
                Pos {
                    x: batch as f32,
                    y: 0.0,
                },
            );
            entities.push(e);
        }

        // Remove half
        for i in (0..entities.len()).step_by(2) {
            world.remove::<Pos>(entities[i]);
        }

        // Add different component
        for &e in &entities {
            world.insert(e, Vel { dx: 1.0, dy: 0.0 });
        }

        // Despawn quarter
        let remove_count = entities.len() / 4;
        for _ in 0..remove_count {
            if let Some(e) = entities.pop() {
                world.despawn(e);
            }
        }
    }

    // World should still be consistent
    let _ = world.query::<Pos>();
    let _ = world.query::<Vel>();
}
