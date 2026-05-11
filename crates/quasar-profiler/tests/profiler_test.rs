//! Tests for the public quasar-profiler API.

use quasar_profiler::{
    ChromeTraceExport, CpuProfiler, FrameGuard, MemoryTracker, ScopedTimer, TimingRecord,
};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[test]
fn cpu_profiler_records_frame_timing() {
    let profiler = CpuProfiler::new();
    let frame_start = Instant::now();

    profiler.begin_frame();
    std::thread::sleep(Duration::from_millis(1));
    profiler.end_frame(frame_start);

    let stats = profiler.frame_statistics();
    assert_eq!(stats.count, 1);
    assert!(stats.max >= Duration::from_millis(1));
}

#[test]
fn cpu_profiler_records_scope_timing() {
    let profiler = CpuProfiler::new();
    let scope = profiler.register_scope("test_scope");
    let start = profiler.begin_scope(scope);

    std::thread::sleep(Duration::from_millis(1));
    profiler.end_scope(scope, start);

    let stats = profiler.scope_statistics(scope);
    assert_eq!(stats.count, 1);
    assert!(stats.max >= Duration::from_millis(1));
}

#[test]
fn scoped_timer_records_on_drop() {
    let profiler = Arc::new(CpuProfiler::new());

    {
        let _timer = ScopedTimer::with_profiler(Arc::clone(&profiler), "drop_scope");
        std::thread::sleep(Duration::from_millis(1));
    }

    let scope = profiler
        .scopes()
        .into_iter()
        .find_map(|(id, info)| (info.name == "drop_scope").then_some(id))
        .expect("scope should be registered");
    assert_eq!(profiler.scope_statistics(scope).count, 1);
}

#[test]
fn frame_guard_records_on_drop() {
    let profiler = Arc::new(CpuProfiler::new());

    {
        let _guard = FrameGuard::with_profiler(Arc::clone(&profiler));
        std::thread::sleep(Duration::from_millis(1));
    }

    assert_eq!(profiler.frame_statistics().count, 1);
}

#[test]
fn timing_record_duration_saturates_on_invalid_order() {
    let record = TimingRecord {
        scope_id: quasar_profiler::ScopeId(1),
        start_ns: 100,
        end_ns: 50,
        thread_id: 0,
        frame: 0,
    };

    assert_eq!(record.duration(), Duration::ZERO);
}

#[test]
fn chrome_trace_export_uses_scope_names() {
    let profiler = CpuProfiler::new();
    let scope = profiler.register_scope("trace_scope");
    let start = profiler.begin_scope(scope);
    profiler.end_scope(scope, start);

    let export = ChromeTraceExport::from_cpu_profiler(&profiler.records(), &profiler.scopes());
    let json = export.to_json_string();

    assert!(json.contains("trace_scope"));
}

#[test]
fn memory_tracker_records_manual_allocations() {
    let tracker = MemoryTracker::new();
    let layout = std::alloc::Layout::new::<u64>();
    let allocation = tracker.record_allocation(layout);

    assert_eq!(tracker.statistics().live_allocations, 1);

    tracker.record_deallocation(allocation);
    assert_eq!(tracker.statistics().live_allocations, 0);
}
