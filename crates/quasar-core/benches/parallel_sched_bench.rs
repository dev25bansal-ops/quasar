//! Benchmarks comparing sequential vs parallel system scheduling.
//!
//! Run with: `cargo bench -p quasar-core --features parallel --bench parallel_sched_bench`

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use criterion::{criterion_group, criterion_main, Criterion};
#[cfg(feature = "parallel")]
use quasar_core::ecs::parallel::SystemAccess;
use quasar_core::ecs::parallel::{ComponentAccess, SystemGraph, SystemNode};
use quasar_core::ecs::{Schedule, SystemStage, World};

/// Create a CPU-heavy system that does some dummy computation.
fn make_heavy_system(name: &str, work: Arc<AtomicU64>) -> Box<dyn quasar_core::ecs::System> {
    let work_clone = work.clone();
    Box::new(quasar_core::ecs::FnSystem::new(name, move |_| {
        // Simulate heavy work
        let mut sum: u64 = 0;
        for i in 0u64..10_000 {
            sum = sum.wrapping_add(i.wrapping_mul(7).wrapping_rem(13));
        }
        work_clone.fetch_add(sum, Ordering::Relaxed);
    }))
}

/// Create a lightweight system with minimal work.
fn make_light_system(name: &str, counter: Arc<AtomicU64>) -> Box<dyn quasar_core::ecs::System> {
    let counter_clone = counter.clone();
    Box::new(quasar_core::ecs::FnSystem::new(name, move |_| {
        counter_clone.fetch_add(1, Ordering::Relaxed);
    }))
}

// ---------------------------------------------------------------------------
// Benchmark: Parallel vs Sequential with non-conflicting systems
// ---------------------------------------------------------------------------

fn bench_parallel_non_conflicting(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_vs_sequential");

    // Create 8 systems that read different types (no conflicts)
    let counters: Vec<Arc<AtomicU64>> = (0..8).map(|_| Arc::new(AtomicU64::new(0))).collect();

    group.bench_function("sequential_8_no_conflict", |b| {
        let mut schedule = Schedule::new();
        for (i, counter) in counters.iter().enumerate() {
            schedule.add_system(
                SystemStage::Update,
                make_light_system(&format!("light_{}", i), counter.clone()),
            );
        }
        let mut world = World::new();
        b.iter(|| {
            // Reset counters
            for ctr in &counters {
                ctr.store(0, Ordering::Relaxed);
            }
            schedule.run(&mut world);
        });
    });

    group.bench_function("parallel_8_no_conflict", |b| {
        let mut graph = SystemGraph::new(SystemStage::Update);
        let counters: Vec<Arc<AtomicU64>> = (0..8).map(|_| Arc::new(AtomicU64::new(0))).collect();
        for (i, counter) in counters.iter().enumerate() {
            graph.add_system(
                SystemNode::new(make_light_system(&format!("light_{}", i), counter.clone()))
                    .with_component_access(ComponentAccess::read::<u64>()),
            );
        }

        let mut world = World::new();
        b.iter(|| {
            for ctr in &counters {
                ctr.store(0, Ordering::Relaxed);
            }
            graph.run_parallel(&mut world);
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark: Schedule with access declarations (parallel enabled)
// ---------------------------------------------------------------------------

fn bench_schedule_parallel_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("schedule_parallel");

    group.bench_function("4_systems_with_access", |b| {
        let counters: Vec<Arc<AtomicU64>> = (0..4).map(|_| Arc::new(AtomicU64::new(0))).collect();

        b.iter(|| {
            let mut schedule = Schedule::new();
            #[cfg(feature = "parallel")]
            {
                schedule.add_system_with_access(
                    SystemStage::Update,
                    make_light_system("read_a", counters[0].clone()),
                    SystemAccess::new().read::<i32>(),
                );
                schedule.add_system_with_access(
                    SystemStage::Update,
                    make_light_system("read_b", counters[1].clone()),
                    SystemAccess::new().read::<f32>(),
                );
                schedule.add_system_with_access(
                    SystemStage::Update,
                    make_light_system("read_c", counters[2].clone()),
                    SystemAccess::new().read::<String>(),
                );
                schedule.add_system_with_access(
                    SystemStage::Update,
                    make_light_system("read_d", counters[3].clone()),
                    SystemAccess::new().read::<u64>(),
                );
            }
            #[cfg(not(feature = "parallel"))]
            {
                schedule.add_system(
                    SystemStage::Update,
                    make_light_system("read_a", counters[0].clone()),
                );
                schedule.add_system(
                    SystemStage::Update,
                    make_light_system("read_b", counters[1].clone()),
                );
                schedule.add_system(
                    SystemStage::Update,
                    make_light_system("read_c", counters[2].clone()),
                );
                schedule.add_system(
                    SystemStage::Update,
                    make_light_system("read_d", counters[3].clone()),
                );
            }

            let mut world = World::new();
            schedule.run(&mut world);
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark: Conflict graph construction
// ---------------------------------------------------------------------------

fn bench_conflict_graph(c: &mut Criterion) {
    let mut group = c.benchmark_group("conflict_graph");

    group.bench_function("build_16_systems_mixed_conflicts", |b| {
        b.iter(|| {
            let mut graph = SystemGraph::new(SystemStage::Update);
            for i in 0..16 {
                let counter = Arc::new(AtomicU64::new(0));
                let node = if i % 2 == 0 {
                    SystemNode::new(make_light_system(&format!("sys_{}", i), counter))
                        .with_component_access(ComponentAccess::read::<i32>())
                } else {
                    SystemNode::new(make_light_system(&format!("sys_{}", i), counter))
                        .with_component_access(ComponentAccess::write::<i32>())
                };
                graph.add_system(node);
            }

            graph.build_dependencies();
            let _batches = graph.topological_groups();
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark: Heavy CPU work parallelized
// ---------------------------------------------------------------------------

fn bench_heavy_cpu_parallel(c: &mut Criterion) {
    let mut group = c.benchmark_group("heavy_cpu");

    group.bench_function("4_heavy_parallel", |b| {
        let work_items: Vec<Arc<AtomicU64>> = (0..4).map(|_| Arc::new(AtomicU64::new(0))).collect();

        b.iter(|| {
            let mut graph = SystemGraph::new(SystemStage::Update);
            for (i, work) in work_items.iter().enumerate() {
                // Each system reads a different type → no conflicts
                let access = match i % 4 {
                    0 => ComponentAccess::read::<i32>(),
                    1 => ComponentAccess::read::<f32>(),
                    2 => ComponentAccess::read::<String>(),
                    _ => ComponentAccess::read::<u64>(),
                };
                graph.add_system(
                    SystemNode::new(make_heavy_system(&format!("heavy_{}", i), work.clone()))
                        .with_component_access(access),
                );
            }

            let mut world = World::new();
            graph.run_parallel(&mut world);
        });
    });

    group.bench_function("4_heavy_sequential", |b| {
        let work_items: Vec<Arc<AtomicU64>> = (0..4).map(|_| Arc::new(AtomicU64::new(0))).collect();

        b.iter(|| {
            let mut graph = SystemGraph::new(SystemStage::Update);
            for (i, work) in work_items.iter().enumerate() {
                let access = match i % 4 {
                    0 => ComponentAccess::read::<i32>(),
                    1 => ComponentAccess::read::<f32>(),
                    2 => ComponentAccess::read::<String>(),
                    _ => ComponentAccess::read::<u64>(),
                };
                graph.add_system(
                    SystemNode::new(make_heavy_system(&format!("heavy_{}", i), work.clone()))
                        .with_component_access(access),
                );
            }

            let mut world = World::new();
            graph.run_sequential(&mut world);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_parallel_non_conflicting,
    bench_schedule_parallel_access,
    bench_conflict_graph,
    bench_heavy_cpu_parallel,
);
criterion_main!(benches);
