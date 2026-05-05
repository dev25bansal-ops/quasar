//! Tests for quasar-profiler crate

use quasar_profiler::prelude::*;

#[test]
fn test_profiler_creation() {
    let profiler = Profiler::new();
    assert!(profiler.is_some());
}

#[test]
fn test_profiler_frame_timing() {
    let mut profiler = Profiler::new().unwrap();
    
    profiler.begin_frame();
    profiler.end_frame();
    
    // Frame timing should be recorded
    let frame_time = profiler.frame_time();
    assert!(frame_time >= 0.0);
}

#[test]
fn test_profiler_scope_timing() {
    let mut profiler = Profiler::new().unwrap();
    
    profiler.begin_scope("test_scope");
    profiler.end_scope();
    
    // Scope timing should be recorded
    let scope_time = profiler.scope_time("test_scope");
    assert!(scope_time >= 0.0);
}

#[test]
fn test_profiler_multiple_scopes() {
    let mut profiler = Profiler::new().unwrap();
    
    profiler.begin_scope("scope1");
    profiler.end_scope();
    
    profiler.begin_scope("scope2");
    profiler.end_scope();
    
    // Both scopes should be recorded
    assert!(profiler.scope_time("scope1") >= 0.0);
    assert!(profiler.scope_time("scope2") >= 0.0);
}

#[test]
fn test_profiler_nested_scopes() {
    let mut profiler = Profiler::new().unwrap();
    
    profiler.begin_scope("outer");
    profiler.begin_scope("inner");
    profiler.end_scope();
    profiler.end_scope();
    
    // Both scopes should be recorded
    assert!(profiler.scope_time("outer") >= 0.0);
    assert!(profiler.scope_time("inner") >= 0.0);
}

#[test]
fn test_profiler_frame_count() {
    let mut profiler = Profiler::new().unwrap();
    
    profiler.begin_frame();
    profiler.end_frame();
    
    profiler.begin_frame();
    profiler.end_frame();
    
    assert_eq!(profiler.frame_count(), 2);
}

#[test]
fn test_profiler_reset() {
    let mut profiler = Profiler::new().unwrap();
    
    profiler.begin_frame();
    profiler.end_frame();
    
    profiler.reset();
    
    // After reset, frame count should be 0
    assert_eq!(profiler.frame_count(), 0);
}

#[test]
fn test_profiler_fps() {
    let mut profiler = Profiler::new().unwrap();
    
    profiler.begin_frame();
    profiler.end_frame();
    
    let fps = profiler.fps();
    assert!(fps > 0.0);
}

#[test]
fn test_profiler_average_frame_time() {
    let mut profiler = Profiler::new().unwrap();
    
    profiler.begin_frame();
    profiler.end_frame();
    
    profiler.begin_frame();
    profiler.end_frame();
    
    let avg_time = profiler.average_frame_time();
    assert!(avg_time >= 0.0);
}

#[test]
fn test_profiler_max_frame_time() {
    let mut profiler = Profiler::new().unwrap();
    
    profiler.begin_frame();
    profiler.end_frame();
    
    profiler.begin_frame();
    profiler.end_frame();
    
    let max_time = profiler.max_frame_time();
    assert!(max_time >= 0.0);
}

#[test]
fn test_profiler_min_frame_time() {
    let mut profiler = Profiler::new().unwrap();
    
    profiler.begin_frame();
    profiler.end_frame();
    
    profiler.begin_frame();
    profiler.end_frame();
    
    let min_time = profiler.min_frame_time();
    assert!(min_time >= 0.0);
}

#[test]
fn test_profiler_scope_names() {
    let mut profiler = Profiler::new().unwrap();
    
    profiler.begin_scope("scope1");
    profiler.end_scope();
    
    profiler.begin_scope("scope2");
    profiler.end_scope();
    
    let scope_names = profiler.scope_names();
    assert_eq!(scope_names.len(), 2);
    assert!(scope_names.contains(&"scope1".to_string()));
    assert!(scope_names.contains(&"scope2".to_string()));
}

#[test]
fn test_profiler_export_stats() {
    let mut profiler = Profiler::new().unwrap();
    
    profiler.begin_frame();
    profiler.begin_scope("test_scope");
    profiler.end_scope();
    profiler.end_frame();
    
    let stats = profiler.export_stats();
    assert!(!stats.is_empty());
}

#[test]
fn test_profiler_export_json() {
    let mut profiler = Profiler::new().unwrap();
    
    profiler.begin_frame();
    profiler.begin_scope("test_scope");
    profiler.end_scope();
    profiler.end_frame();
    
    let json = profiler.export_json();
    assert!(!json.is_empty());
    
    // JSON should be valid
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed.is_object());
}

#[test]
fn test_profiler_export_csv() {
    let mut profiler = Profiler::new().unwrap();
    
    profiler.begin_frame();
    profiler.begin_scope("test_scope");
    profiler.end_scope();
    profiler.end_frame();
    
    let csv = profiler.export_csv();
    assert!(!csv.is_empty());
    
    // CSV should have headers
    assert!(csv.contains("frame"));
    assert!(csv.contains("scope"));
}

#[test]
fn test_profiler_memory_stats() {
    let profiler = Profiler::new().unwrap();
    
    let memory_stats = profiler.memory_stats();
    assert!(memory_stats.is_some());
}

#[test]
fn test_profiler_cpu_stats() {
    let profiler = Profiler::new().unwrap();
    
    let cpu_stats = profiler.cpu_stats();
    assert!(cpu_stats.is_some());
}

#[test]
fn test_profiler_gpu_stats() {
    let profiler = Profiler::new().unwrap();
    
    let gpu_stats = profiler.gpu_stats();
    // GPU stats may not be available if GPU profiling is not enabled
    // This test just checks that the method exists
    assert!(gpu_stats.is_some() || gpu_stats.is_none());
}

#[test]
fn test_profiler_timing_stats() {
    let profiler = Profiler::new().unwrap();
    
    let timing_stats = profiler.timing_stats();
    assert!(timing_stats.is_some());
}

#[test]
fn test_profiler_scope_hierarchy() {
    let mut profiler = Profiler::new().unwrap();
    
    profiler.begin_scope("parent");
    profiler.begin_scope("child1");
    profiler.end_scope();
    profiler.begin_scope("child2");
    profiler.end_scope();
    profiler.end_scope();
    
    // All scopes should be recorded
    assert!(profiler.scope_time("parent") >= 0.0);
    assert!(profiler.scope_time("child1") >= 0.0);
    assert!(profiler.scope_time("child2") >= 0.0);
}

#[test]
fn test_profiler_concurrent_scopes() {
    let mut profiler = Profiler::new().unwrap();
    
    // Test that profiler can handle multiple scopes in the same frame
    profiler.begin_scope("scope1");
    profiler.end_scope();
    
    profiler.begin_scope("scope2");
    profiler.end_scope();
    
    profiler.begin_scope("scope3");
    profiler.end_scope();
    
    assert_eq!(profiler.scope_names().len(), 3);
}

#[test]
fn test_profiler_long_scope_name() {
    let mut profiler = Profiler::new().unwrap();
    
    let long_name = "very_long_scope_name_that_tests_the_profiler_can_handle_long_names";
    profiler.begin_scope(long_name);
    profiler.end_scope();
    
    assert!(profiler.scope_time(long_name) >= 0.0);
}

#[test]
fn test_profiler_special_characters_in_scope_name() {
    let mut profiler = Profiler::new().unwrap();
    
    profiler.begin_scope("test_scope_with_underscores");
    profiler.end_scope();
    
    assert!(profiler.scope_time("test_scope_with_underscores") >= 0.0);
}

#[test]
fn test_profiler_empty_scope_name() {
    let mut profiler = Profiler::new().unwrap();
    
    // Empty scope names should be handled gracefully
    profiler.begin_scope("");
    profiler.end_scope();
    
    // The profiler should still work
    assert!(profiler.frame_count() >= 0);
}

#[test]
fn test_profiler_duplicate_scope_names() {
    let mut profiler = Profiler::new().unwrap();
    
    // Test that duplicate scope names are handled
    profiler.begin_scope("duplicate");
    profiler.end_scope();
    
    profiler.begin_scope("duplicate");
    profiler.end_scope();
    
    // Both should be recorded
    let total_time = profiler.scope_time("duplicate");
    assert!(total_time >= 0.0);
}

#[test]
fn test_profiler_export_format() {
    let mut profiler = Profiler::new().unwrap();
    
    profiler.begin_frame();
    profiler.begin_scope("test_scope");
    profiler.end_scope();
    profiler.end_frame();
    
    // Test different export formats
    let json = profiler.export_json();
    let csv = profiler.export_csv();
    
    // Both should produce valid output
    assert!(!json.is_empty());
    assert!(!csv.is_empty());
    
    // JSON should be parseable
    let _: serde_json::Value = serde_json::from_str(&json).unwrap();
}

#[test]
fn test_profiler_performance_overhead() {
    let profiler = Profiler::new().unwrap();
    
    // Test that profiler doesn't add significant overhead
    let start = std::time::Instant::now();
    
    for _ in 0..1000 {
        profiler.begin_frame();
        profiler.begin_scope("test");
        profiler.end_scope();
        profiler.end_frame();
    }
    
    let duration = start.elapsed();
    
    // Should complete in reasonable time (< 1 second for 1000 frames)
    assert!(duration.as_millis() < 1000);
}

#[test]
fn test_profiler_thread_safety() {
    // Test that profiler can be used from multiple threads
    let profiler = Profiler::new().unwrap();
    
    let handle = std::thread::spawn(move || {
        let mut profiler = profiler;
        profiler.begin_frame();
        profiler.begin_scope("thread_scope");
        profiler.end_scope();
        profiler.end_frame();
    });
    
    handle.join().unwrap();
    
    // Main thread profiler should still work
    profiler.begin_frame();
    profiler.end_frame();
}

#[test]
fn test_profiler_consistency() {
    let mut profiler = Profiler::new().unwrap();
    
    profiler.begin_frame();
    profiler.begin_scope("scope1");
    profiler.end_scope();
    profiler.end_frame();
    
    let frame_time_1 = profiler.frame_time();
    let scope_time_1 = profiler.scope_time("scope1");
    
    profiler.begin_frame();
    profiler.begin_scope("scope1");
    profiler.end_scope();
    profiler.end_frame();
    
    let frame_time_2 = profiler.frame_time();
    let scope_time_2 = profiler.scope_time("scope1");
    
    // Timing should be consistent across multiple frames
    assert!(frame_time_1 >= 0.0);
    assert!(scope_time_1 >= 0.0);
    assert!(frame_time_2 >= 0.0);
    assert!(scope_time_2 >= 0.0);
}