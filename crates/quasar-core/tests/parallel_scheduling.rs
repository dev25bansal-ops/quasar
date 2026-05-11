//! Integration tests for parallel system scheduling.
//!
//! These tests verify that:
//! 1. Conflict detection correctly identifies data races
//! 2. Parallel batches contain only non-conflicting systems
//! 3. Execution order respects explicit ordering constraints
//! 4. Results are correct regardless of parallel vs sequential execution

use quasar_core::ecs::parallel::{
    system_node, system_node_with_access, ComponentAccess, ParallelSchedule, SystemAccess,
    SystemGraph, SystemNode,
};
use quasar_core::ecs::{FnSystem, Schedule, System, SystemStage, World};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Helper: create a system that increments an atomic counter
// ---------------------------------------------------------------------------

fn make_counter_system(name: &str, counter: Arc<AtomicUsize>) -> Box<dyn System> {
    let counter_clone = counter.clone();
    Box::new(FnSystem::new(name, move |_| {
        counter_clone.fetch_add(1, Ordering::SeqCst);
    }))
}

// ---------------------------------------------------------------------------
// Conflict detection tests
// ---------------------------------------------------------------------------

#[test]
fn test_conflict_write_write() {
    // Two systems writing the same type must conflict
    let access_a = SystemAccess::new().write::<i32>();
    let access_b = SystemAccess::new().write::<i32>();
    assert!(access_a.conflicts_with(&access_b));
}

#[test]
fn test_conflict_read_write() {
    // One system reading, another writing the same type must conflict
    let access_a = SystemAccess::new().read::<i32>();
    let access_b = SystemAccess::new().write::<i32>();
    assert!(access_a.conflicts_with(&access_b));
    assert!(access_b.conflicts_with(&access_a));
}

#[test]
fn test_no_conflict_read_read() {
    // Two systems reading the same type do NOT conflict
    let access_a = SystemAccess::new().read::<i32>();
    let access_b = SystemAccess::new().read::<i32>();
    assert!(!access_a.conflicts_with(&access_b));
}

#[test]
fn test_no_conflict_different_types() {
    // Systems operating on different types do NOT conflict
    let access_a = SystemAccess::new().write::<i32>();
    let access_b = SystemAccess::new().write::<f32>();
    assert!(!access_a.conflicts_with(&access_b));
}

#[test]
fn test_cross_conflict() {
    // A: reads Position, writes Velocity
    // B: reads Velocity, writes Position
    // → CONFLICT: A writes what B reads, B writes what A reads
    let access_a = SystemAccess::new().read::<i32>().write::<f32>();
    let access_b = SystemAccess::new().read::<f32>().write::<i32>();
    assert!(access_a.conflicts_with(&access_b));
}

// ---------------------------------------------------------------------------
// SystemGraph parallel execution tests
// ---------------------------------------------------------------------------

#[test]
fn test_parallel_batch_execution() {
    let counter_a = Arc::new(AtomicUsize::new(0));
    let counter_b = Arc::new(AtomicUsize::new(0));

    let mut graph = SystemGraph::new(SystemStage::Update);
    graph.add_system(
        SystemNode::new(make_counter_system("sys_a", counter_a.clone()))
            .with_component_access(ComponentAccess::read::<i32>()),
    );
    graph.add_system(
        SystemNode::new(make_counter_system("sys_b", counter_b.clone()))
            .with_component_access(ComponentAccess::read::<f32>()),
    );

    let mut world = World::new();
    graph.run_parallel(&mut world);

    // Both systems should have run
    assert_eq!(counter_a.load(Ordering::SeqCst), 1);
    assert_eq!(counter_b.load(Ordering::SeqCst), 1);
}

#[test]
fn test_conflicting_systems_run_sequentially() {
    // Two conflicting systems should run in separate batches
    let counter_a = Arc::new(AtomicUsize::new(0));
    let counter_b = Arc::new(AtomicUsize::new(0));

    let mut graph = SystemGraph::new(SystemStage::Update);
    graph.add_system(
        SystemNode::new(make_counter_system("writer", counter_a.clone()))
            .with_component_access(ComponentAccess::write::<i32>()),
    );
    graph.add_system(
        SystemNode::new(make_counter_system("reader", counter_b.clone()))
            .with_component_access(ComponentAccess::read::<i32>()),
    );

    let mut world = World::new();
    graph.run_parallel(&mut world);

    assert_eq!(counter_a.load(Ordering::SeqCst), 1);
    assert_eq!(counter_b.load(Ordering::SeqCst), 1);
}

#[test]
fn test_three_parallel_systems() {
    // Three systems reading different types → all should run in parallel
    let counter_a = Arc::new(AtomicUsize::new(0));
    let counter_b = Arc::new(AtomicUsize::new(0));
    let counter_c = Arc::new(AtomicUsize::new(0));

    let mut graph = SystemGraph::new(SystemStage::Update);
    graph.add_system(
        SystemNode::new(make_counter_system("read_a", counter_a.clone()))
            .with_component_access(ComponentAccess::read::<i32>()),
    );
    graph.add_system(
        SystemNode::new(make_counter_system("read_b", counter_b.clone()))
            .with_component_access(ComponentAccess::read::<f32>()),
    );
    graph.add_system(
        SystemNode::new(make_counter_system("read_c", counter_c.clone()))
            .with_component_access(ComponentAccess::read::<String>()),
    );

    let mut world = World::new();
    graph.run_parallel(&mut world);

    assert_eq!(counter_a.load(Ordering::SeqCst), 1);
    assert_eq!(counter_b.load(Ordering::SeqCst), 1);
    assert_eq!(counter_c.load(Ordering::SeqCst), 1);
}

// ---------------------------------------------------------------------------
// ParallelSchedule tests
// ---------------------------------------------------------------------------

#[test]
fn test_parallel_schedule_multiple_stages() {
    let counter = Arc::new(AtomicUsize::new(0));

    let mut schedule = ParallelSchedule::new();
    schedule.add_system(
        SystemStage::PreUpdate,
        SystemNode::new(make_counter_system("pre", counter.clone()))
            .with_component_access(ComponentAccess::read::<i32>()),
    );
    schedule.add_system(
        SystemStage::Update,
        SystemNode::new(make_counter_system("update", counter.clone()))
            .with_component_access(ComponentAccess::read::<f32>()),
    );
    schedule.add_system(
        SystemStage::PostUpdate,
        SystemNode::new(make_counter_system("post", counter.clone()))
            .with_component_access(ComponentAccess::read::<String>()),
    );

    let mut world = World::new();
    schedule.run(&mut world);

    assert_eq!(counter.load(Ordering::SeqCst), 3);
}

#[test]
fn test_parallel_schedule_toggle_off() {
    let counter = Arc::new(AtomicUsize::new(0));

    let mut schedule = ParallelSchedule::new();
    schedule.add_system(
        SystemStage::Update,
        SystemNode::new(make_counter_system("sys", counter.clone()))
            .with_component_access(ComponentAccess::read::<i32>()),
    );
    schedule.set_parallel(false);

    let mut world = World::new();
    schedule.run(&mut world);

    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

// ---------------------------------------------------------------------------
// Schedule with parallel feature tests
// ---------------------------------------------------------------------------

#[test]
fn test_schedule_with_access_parallel() {
    let counter_a = Arc::new(AtomicUsize::new(0));
    let counter_b = Arc::new(AtomicUsize::new(0));

    let mut schedule = Schedule::new();
    #[cfg(feature = "parallel")]
    {
        schedule.add_system_with_access(
            SystemStage::Update,
            make_counter_system("sys_a", counter_a.clone()),
            SystemAccess::new().read::<i32>(),
        );
        schedule.add_system_with_access(
            SystemStage::Update,
            make_counter_system("sys_b", counter_b.clone()),
            SystemAccess::new().read::<f32>(),
        );
    }
    #[cfg(not(feature = "parallel"))]
    {
        schedule.add_system(
            SystemStage::Update,
            make_counter_system("sys_a", counter_a.clone()),
        );
        schedule.add_system(
            SystemStage::Update,
            make_counter_system("sys_b", counter_b.clone()),
        );
    }

    let mut world = World::new();
    schedule.run(&mut world);

    assert_eq!(counter_a.load(Ordering::SeqCst), 1);
    assert_eq!(counter_b.load(Ordering::SeqCst), 1);
}

#[test]
fn test_schedule_with_access_conflicting() {
    // Two conflicting systems — both should still run, just sequentially
    let counter_a = Arc::new(AtomicUsize::new(0));
    let counter_b = Arc::new(AtomicUsize::new(0));

    let mut schedule = Schedule::new();
    #[cfg(feature = "parallel")]
    {
        schedule.add_system_with_access(
            SystemStage::Update,
            make_counter_system("writer", counter_a.clone()),
            SystemAccess::new().write::<i32>(),
        );
        schedule.add_system_with_access(
            SystemStage::Update,
            make_counter_system("reader", counter_b.clone()),
            SystemAccess::new().read::<i32>(),
        );
    }
    #[cfg(not(feature = "parallel"))]
    {
        schedule.add_system(
            SystemStage::Update,
            make_counter_system("writer", counter_a.clone()),
        );
        schedule.add_system(
            SystemStage::Update,
            make_counter_system("reader", counter_b.clone()),
        );
    }

    let mut world = World::new();
    schedule.run(&mut world);

    assert_eq!(counter_a.load(Ordering::SeqCst), 1);
    assert_eq!(counter_b.load(Ordering::SeqCst), 1);
}

#[test]
fn test_schedule_mixed_access_and_non_access() {
    // Mix systems with access declarations and systems without
    let counter_a = Arc::new(AtomicUsize::new(0));
    let counter_b = Arc::new(AtomicUsize::new(0));

    let mut schedule = Schedule::new();
    #[cfg(feature = "parallel")]
    {
        schedule.add_system_with_access(
            SystemStage::Update,
            make_counter_system("with_access", counter_a.clone()),
            SystemAccess::new().read::<i32>(),
        );
    }
    #[cfg(not(feature = "parallel"))]
    {
        schedule.add_system(
            SystemStage::Update,
            make_counter_system("with_access", counter_a.clone()),
        );
    }
    schedule.add_system(
        SystemStage::Update,
        make_counter_system("without_access", counter_b.clone()),
    );

    let mut world = World::new();
    schedule.run(&mut world);

    assert_eq!(counter_a.load(Ordering::SeqCst), 1);
    assert_eq!(counter_b.load(Ordering::SeqCst), 1);
}

// ---------------------------------------------------------------------------
// Ordering constraint tests
// ---------------------------------------------------------------------------

#[test]
fn test_explicit_ordering_preserved() {
    // Even if two systems don't conflict, explicit ordering should be respected
    let order = Arc::new(std::sync::Mutex::new(Vec::new()));

    let order_a = order.clone();
    let order_b = order.clone();

    let sys_a = FnSystem::new("first", move |_| {
        order_a.lock().unwrap().push("first");
    });
    let sys_b = FnSystem::new("second", move |_| {
        order_b.lock().unwrap().push("second");
    });

    let mut schedule = Schedule::new();
    #[cfg(feature = "parallel")]
    {
        schedule.add_system_with_access(
            SystemStage::Update,
            Box::new(sys_a),
            SystemAccess::new().read::<i32>(),
        );
        schedule.add_system_with_access(
            SystemStage::Update,
            Box::new(sys_b),
            SystemAccess::new().read::<f32>(),
        );
        schedule.add_order("first", "second");
    }
    #[cfg(not(feature = "parallel"))]
    {
        schedule.add_system(SystemStage::Update, Box::new(sys_a));
        schedule.add_system(SystemStage::Update, Box::new(sys_b));
        schedule.add_order("first", "second");
    }

    let mut world = World::new();
    schedule.run(&mut world);

    let final_order = order.lock().unwrap();
    assert_eq!(*final_order, vec!["first", "second"]);
}

// ---------------------------------------------------------------------------
// Resource access conflict tests
// ---------------------------------------------------------------------------

#[test]
fn test_resource_write_conflict() {
    let access_a = SystemAccess::new().res_write::<i32>();
    let access_b = SystemAccess::new().res_read::<i32>();
    assert!(access_a.conflicts_with(&access_b));
}

#[test]
fn test_resource_read_read_no_conflict() {
    let access_a = SystemAccess::new().res_read::<i32>();
    let access_b = SystemAccess::new().res_read::<i32>();
    assert!(!access_a.conflicts_with(&access_b));
}

// ---------------------------------------------------------------------------
// Empty and edge case tests
// ---------------------------------------------------------------------------

#[test]
fn test_empty_schedule_runs_without_panic() {
    let mut schedule = Schedule::new();
    let mut world = World::new();
    schedule.run(&mut world); // Should not panic
}

#[test]
fn test_empty_parallel_schedule_runs_without_panic() {
    let mut schedule = ParallelSchedule::new();
    let mut world = World::new();
    schedule.run(&mut world); // Should not panic
}

#[test]
fn test_empty_graph_runs_without_panic() {
    let graph = SystemGraph::new(SystemStage::Update);
    // Can't run an empty graph directly, but verify no panic in construction
    assert!(graph.is_empty());
    assert_eq!(graph.len(), 0);
}

// ---------------------------------------------------------------------------
// SystemNode builder tests
// ---------------------------------------------------------------------------

#[test]
fn test_system_node_builder() {
    let node = system_node(FnSystem::new("test", |_| {}))
        .with_component_access(ComponentAccess::read::<i32>())
        .with_component_access(ComponentAccess::write::<f32>())
        .with_resource_access(ComponentAccess::read::<String>())
        .after("dependency")
        .before("dependent");

    assert_eq!(node.system.name(), "test");
    assert!(node
        .component_reads
        .contains(&std::any::TypeId::of::<i32>()));
    assert!(node
        .component_writes
        .contains(&std::any::TypeId::of::<f32>()));
    assert!(node
        .resource_reads
        .contains(&std::any::TypeId::of::<String>()));
    assert_eq!(node.after, vec!["dependency"]);
    assert_eq!(node.before, vec!["dependent"]);
}

#[test]
fn test_system_node_with_access_macro() {
    let node = system_node_with_access::<_, (i32,), (f32,)>(FnSystem::new("typed", |_| {}));

    assert!(node
        .component_reads
        .contains(&std::any::TypeId::of::<i32>()));
    assert!(node
        .component_writes
        .contains(&std::any::TypeId::of::<f32>()));
}

// ---------------------------------------------------------------------------
// ParallelBatch tests
// ---------------------------------------------------------------------------

#[test]
fn test_parallel_batch_detection() {
    use quasar_core::ecs::parallel::ParallelBatch;

    let single = ParallelBatch::new(vec![0]);
    assert!(!single.is_parallel());
    assert_eq!(single.len(), 1);

    let multi = ParallelBatch::new(vec![0, 1, 2]);
    assert!(multi.is_parallel());
    assert_eq!(multi.len(), 3);

    let empty = ParallelBatch::new(vec![]);
    assert!(!empty.is_parallel());
    assert_eq!(empty.len(), 0);
}

// ---------------------------------------------------------------------------
// ConflictGraph tests
// ---------------------------------------------------------------------------

#[test]
fn test_conflict_graph_symmetric() {
    use quasar_core::ecs::parallel::ConflictGraph;

    let mut cg = ConflictGraph::new(4);
    cg.add_conflict(1, 3);

    assert!(cg.has_conflict(1, 3));
    assert!(cg.has_conflict(3, 1));
    assert!(!cg.has_conflict(0, 1));
    assert!(!cg.has_conflict(0, 2));
    assert!(!cg.has_conflict(2, 3));
}

#[test]
fn test_conflict_graph_non_conflicting() {
    use quasar_core::ecs::parallel::ConflictGraph;

    let mut cg = ConflictGraph::new(5);
    cg.add_conflict(0, 1);
    cg.add_conflict(0, 2);

    // Given group [0], find systems from {0,1,2,3,4} that don't conflict
    let candidates: Vec<usize> = (0..5).collect();
    let non_conflicting = cg.non_conflicting_with(&[0], candidates.iter().filter(|&&x| x != 0));

    // 3 and 4 don't conflict with 0; 1 and 2 do
    assert!(non_conflicting.contains(&3));
    assert!(non_conflicting.contains(&4));
    assert!(!non_conflicting.contains(&1));
    assert!(!non_conflicting.contains(&2));
}
