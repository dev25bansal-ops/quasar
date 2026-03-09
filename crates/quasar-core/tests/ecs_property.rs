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
