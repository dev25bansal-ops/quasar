use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use quasar_core::ecs::World;

// Lightweight test components.
#[derive(Debug, Clone, Copy)]
struct Position {
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Debug, Clone, Copy)]
struct Velocity {
    dx: f32,
    dy: f32,
    dz: f32,
}

#[derive(Debug, Clone, Copy)]
struct Health(f32);

// ── Spawn benchmarks ─────────────────────────────────────────────

fn bench_spawn(c: &mut Criterion) {
    let mut group = c.benchmark_group("spawn");
    for count in [1_000, 10_000, 100_000] {
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            b.iter(|| {
                let mut world = World::new();
                for i in 0..n {
                    let e = world.spawn();
                    world.insert(
                        e,
                        Position {
                            x: i as f32,
                            y: 0.0,
                            z: 0.0,
                        },
                    );
                    world.insert(
                        e,
                        Velocity {
                            dx: 1.0,
                            dy: 0.0,
                            dz: 0.0,
                        },
                    );
                }
                black_box(&world);
            });
        });
    }
    group.finish();
}

// ── Query iteration benchmarks ───────────────────────────────────

fn bench_query_iter(c: &mut Criterion) {
    let mut group = c.benchmark_group("query_iter");
    for count in [1_000, 10_000, 100_000] {
        let mut world = World::new();
        for i in 0..count {
            let e = world.spawn();
            world.insert(
                e,
                Position {
                    x: i as f32,
                    y: 0.0,
                    z: 0.0,
                },
            );
            world.insert(
                e,
                Velocity {
                    dx: 1.0,
                    dy: 0.0,
                    dz: 0.0,
                },
            );
        }

        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, _| {
            b.iter(|| {
                let results = world.query::<Position>();
                black_box(results.len());
            });
        });
    }
    group.finish();
}

// ── Two-component query benchmarks ───────────────────────────────

fn bench_query2_iter(c: &mut Criterion) {
    let mut group = c.benchmark_group("query2_iter");
    for count in [1_000, 10_000, 100_000] {
        let mut world = World::new();
        for i in 0..count {
            let e = world.spawn();
            world.insert(
                e,
                Position {
                    x: i as f32,
                    y: 0.0,
                    z: 0.0,
                },
            );
            world.insert(
                e,
                Velocity {
                    dx: 1.0,
                    dy: 0.0,
                    dz: 0.0,
                },
            );
        }

        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, _| {
            b.iter(|| {
                let results = world.query2::<Position, Velocity>();
                black_box(results.len());
            });
        });
    }
    group.finish();
}

// ── for_each_mut (simulating a system) ───────────────────────────

fn bench_for_each_mut(c: &mut Criterion) {
    let mut group = c.benchmark_group("for_each_mut");
    for count in [1_000, 10_000, 100_000] {
        let mut world = World::new();
        for i in 0..count {
            let e = world.spawn();
            world.insert(
                e,
                Position {
                    x: i as f32,
                    y: 0.0,
                    z: 0.0,
                },
            );
        }

        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, _| {
            b.iter(|| {
                world.for_each_mut::<Position, _>(|_entity, pos| {
                    pos.x += 1.0;
                });
            });
        });
    }
    group.finish();
}

// ── Despawn benchmarks ───────────────────────────────────────────

fn bench_despawn(c: &mut Criterion) {
    let mut group = c.benchmark_group("despawn");
    for count in [1_000, 10_000] {
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            b.iter_batched(
                || {
                    let mut world = World::new();
                    let mut entities = Vec::with_capacity(n);
                    for i in 0..n {
                        let e = world.spawn();
                        world.insert(
                            e,
                            Position {
                                x: i as f32,
                                y: 0.0,
                                z: 0.0,
                            },
                        );
                        entities.push(e);
                    }
                    (world, entities)
                },
                |(mut world, entities)| {
                    for e in entities {
                        world.despawn(e);
                    }
                    black_box(&world);
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

// ── Fragmented archetype query ───────────────────────────────────

fn bench_fragmented_query(c: &mut Criterion) {
    let mut world = World::new();
    // Create entities with varied component combos to fragment archetypes.
    for i in 0..10_000u32 {
        let e = world.spawn();
        world.insert(
            e,
            Position {
                x: i as f32,
                y: 0.0,
                z: 0.0,
            },
        );
        if i % 2 == 0 {
            world.insert(
                e,
                Velocity {
                    dx: 1.0,
                    dy: 0.0,
                    dz: 0.0,
                },
            );
        }
        if i % 3 == 0 {
            world.insert(e, Health(100.0));
        }
    }

    c.bench_function("fragmented_query_10k", |b| {
        b.iter(|| {
            let results = world.query::<Position>();
            black_box(results.len());
        });
    });
}

criterion_group!(
    benches,
    bench_spawn,
    bench_query_iter,
    bench_query2_iter,
    bench_for_each_mut,
    bench_despawn,
    bench_fragmented_query,
);
criterion_main!(benches);
