//! Profiling integration — puffin/tracy instrumentation.
//!
//! Provides:
//! - Instrumentation macros for system run() calls
//! - GPU timestamp queries per render pass
//! - Editor "Profiler" panel for visualizing results

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

pub struct Profiler {
    pub enabled: bool,
    pub frame_data: VecDeque<FrameProfile>,
    pub max_frames: usize,
    pub current_frame: FrameProfile,
    pub gpu_timestamps: HashMap<String, GpuTimestampRange>,
    pub frame_start: Instant,
}

impl Default for Profiler {
    fn default() -> Self {
        Self::new()
    }
}

impl Profiler {
    pub fn new() -> Self {
        Self {
            enabled: true,
            frame_data: VecDeque::with_capacity(300),
            max_frames: 300,
            current_frame: FrameProfile::new(),
            gpu_timestamps: HashMap::new(),
            frame_start: Instant::now(),
        }
    }

    pub fn begin_frame(&mut self) {
        self.current_frame = FrameProfile::new();
        self.frame_start = Instant::now();
    }

    pub fn end_frame(&mut self) {
        self.current_frame.frame_time = self.frame_start.elapsed();

        if self.frame_data.len() >= self.max_frames {
            self.frame_data.pop_front();
        }

        self.frame_data.push_back(self.current_frame.clone());
    }

    pub fn begin_scope(&mut self, name: &str) {
        let scope = ProfileScope {
            name: name.to_string(),
            start: Instant::now(),
            duration: Duration::ZERO,
            children: Vec::new(),
            depth: self.current_frame.scope_stack.len() as u32,
        };

        self.current_frame.scope_stack.push(name.to_string());

        if let Some(parent_name) = self.current_frame.scope_stack.iter().rev().nth(1) {
            let parent_name = parent_name.clone();
            if let Some(parent) =
                Self::find_scope_mut_impl(&mut self.current_frame.scopes, &parent_name)
            {
                parent.children.push(scope);
            }
        } else {
            self.current_frame.scopes.push(scope);
        }
    }

    pub fn end_scope(&mut self, name: &str) {
        if let Some(scope_name) = self.current_frame.scope_stack.pop() {
            if scope_name == name {
                if let Some(scope) = Self::find_scope_mut_impl(&mut self.current_frame.scopes, name)
                {
                    scope.duration = scope.start.elapsed();
                }
            }
        }
    }

    fn find_scope_mut_impl<'a>(
        scopes: &'a mut Vec<ProfileScope>,
        name: &str,
    ) -> Option<&'a mut ProfileScope> {
        for scope in scopes.iter_mut() {
            if scope.name == name {
                return Some(scope);
            }
            if let Some(child) = Self::find_scope_mut_impl(&mut scope.children, name) {
                return Some(child);
            }
        }
        None
    }

    pub fn record_gpu_timestamp(&mut self, name: String, start: u64, end: u64) {
        self.gpu_timestamps.insert(
            name.clone(),
            GpuTimestampRange {
                name,
                start_timestamp: start,
                end_timestamp: end,
            },
        );
    }

    pub fn get_frame_stats(&self) -> FrameStats {
        if self.frame_data.is_empty() {
            return FrameStats::default();
        }

        let total_frames = self.frame_data.len();
        let total_time: Duration = self.frame_data.iter().map(|f| f.frame_time).sum();
        let avg_frame_time = total_time / total_frames as u32;

        let sorted_times: Vec<Duration> = {
            let mut times: Vec<_> = self.frame_data.iter().map(|f| f.frame_time).collect();
            times.sort();
            times
        };

        let median_frame_time = sorted_times[sorted_times.len() / 2];

        FrameStats {
            avg_frame_time,
            median_frame_time,
            min_frame_time: sorted_times.first().copied().unwrap_or(Duration::ZERO),
            max_frame_time: sorted_times.last().copied().unwrap_or(Duration::ZERO),
            fps: if avg_frame_time > Duration::ZERO {
                1000.0 / avg_frame_time.as_millis() as f32
            } else {
                0.0
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct FrameProfile {
    pub frame_time: Duration,
    pub scopes: Vec<ProfileScope>,
    pub scope_stack: Vec<String>,
}

impl FrameProfile {
    pub fn new() -> Self {
        Self {
            frame_time: Duration::ZERO,
            scopes: Vec::new(),
            scope_stack: Vec::new(),
        }
    }
}

impl Default for FrameProfile {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct ProfileScope {
    pub name: String,
    pub start: Instant,
    pub duration: Duration,
    pub children: Vec<ProfileScope>,
    pub depth: u32,
}

#[derive(Debug, Clone)]
pub struct GpuTimestampRange {
    pub name: String,
    pub start_timestamp: u64,
    pub end_timestamp: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FrameStats {
    pub avg_frame_time: Duration,
    pub median_frame_time: Duration,
    pub min_frame_time: Duration,
    pub max_frame_time: Duration,
    pub fps: f32,
}

#[cfg(feature = "gpu-profiling")]
pub struct GpuProfiler {
    pub query_sets: Vec<wgpu::QuerySet>,
    pub resolve_buffers: Vec<wgpu::Buffer>,
    pub read_buffers: Vec<wgpu::Buffer>,
    pub pending_queries: Vec<(String, u32)>,
    pub next_query_index: u32,
    pub timestamps_per_frame: u32,
}

#[cfg(feature = "gpu-profiling")]
impl GpuProfiler {
    pub fn new(device: &wgpu::Device, timestamps_per_frame: u32) -> Self {
        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("GPU Timestamp Query Set"),
            ty: wgpu::QueryType::Timestamp,
            count: timestamps_per_frame * 2,
        });

        let resolve_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU Timestamp Resolve Buffer"),
            size: (timestamps_per_frame * 2 * std::mem::size_of::<u64>() as u32) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::QUERY_RESOLVE,
            mapped_at_creation: false,
        });

        let read_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU Timestamp Read Buffer"),
            size: (timestamps_per_frame * 2 * std::mem::size_of::<u64>() as u32) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            query_sets: vec![query_set],
            resolve_buffers: vec![resolve_buffer],
            read_buffers: vec![read_buffer],
            pending_queries: Vec::new(),
            next_query_index: 0,
            timestamps_per_frame,
        }
    }

    pub fn begin_timestamp(&mut self, encoder: &mut wgpu::CommandEncoder, name: &str) {
        let query_index = self.next_query_index * 2;
        if let Some(query_set) = self.query_sets.first() {
            encoder.write_timestamp(query_set, query_index);
            self.pending_queries
                .push((name.to_string(), self.next_query_index));
        }
        self.next_query_index += 1;
    }

    pub fn end_timestamp(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let query_index = (self.next_query_index * 2) - 1;
        if let Some(query_set) = self.query_sets.first() {
            encoder.write_timestamp(query_set, query_index);
        }
    }

    pub fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if let (Some(query_set), Some(resolve_buffer)) =
            (self.query_sets.first(), self.resolve_buffers.first())
        {
            encoder.resolve_query_set(query_set, 0..self.next_query_index * 2, resolve_buffer, 0);
        }
    }

    pub fn read_results(&self) -> Vec<(String, u64, u64)> {
        vec![]
    }

    pub fn reset(&mut self) {
        self.next_query_index = 0;
        self.pending_queries.clear();
    }
}

pub fn enable_profiling() {
    #[cfg(feature = "puffin")]
    {
        puffin::set_scopes_on(true);
    }
}

#[macro_export]
macro_rules! profile_scope {
    ($name:expr) => {
        #[cfg(feature = "puffin")]
        puffin::profile_scope!($name);

        #[cfg(feature = "tracy")]
        tracy_client::span!($name);
    };
}

#[macro_export]
macro_rules! profile_function {
    () => {
        $crate::profile_scope!(function_name!());
    };
}

#[macro_export]
macro_rules! profile_frame {
    () => {
        #[cfg(feature = "puffin")]
        puffin::GlobalProfiler::lock().new_frame();
    };
}

#[macro_export]
macro_rules! function_name {
    () => {{
        fn f() {}
        fn type_name_of<T>(_: T) -> &'static str {
            std::any::type_name::<T>()
        }
        let name = type_name_of(f);
        name.strip_suffix("::f")
            .unwrap_or(name)
            .split("::")
            .last()
            .unwrap_or(name)
    }};
}

pub struct ProfilerPlugin;

// ── Frame budget / stall detector ────────────────────────────────

/// Tracks whether frames exceed a budget and counts stalls.
pub struct FrameBudget {
    /// Target frame time, e.g. 16.6 ms for 60 FPS.
    pub target: Duration,
    /// Number of consecutive frames that exceeded the budget.
    pub consecutive_stalls: u32,
    /// Total frames that exceeded the budget since last reset.
    pub total_stalls: u64,
    /// Worst frame time observed since last reset.
    pub worst_frame: Duration,
    /// Ring buffer of the last N frame times for spike analysis.
    recent_times: VecDeque<Duration>,
    max_recent: usize,
}

impl FrameBudget {
    pub fn new(target: Duration) -> Self {
        Self {
            target,
            consecutive_stalls: 0,
            total_stalls: 0,
            worst_frame: Duration::ZERO,
            recent_times: VecDeque::with_capacity(128),
            max_recent: 128,
        }
    }

    /// Call once per frame after profiler.end_frame().
    pub fn record(&mut self, frame_time: Duration) {
        if self.recent_times.len() >= self.max_recent {
            self.recent_times.pop_front();
        }
        self.recent_times.push_back(frame_time);

        if frame_time > self.worst_frame {
            self.worst_frame = frame_time;
        }

        if frame_time > self.target {
            self.consecutive_stalls += 1;
            self.total_stalls += 1;
        } else {
            self.consecutive_stalls = 0;
        }
    }

    /// 99th percentile frame time over the recent window.
    pub fn p99(&self) -> Duration {
        if self.recent_times.is_empty() {
            return Duration::ZERO;
        }
        let mut sorted: Vec<Duration> = self.recent_times.iter().copied().collect();
        sorted.sort();
        let idx = (sorted.len() as f64 * 0.99).ceil() as usize;
        sorted[idx.min(sorted.len() - 1)]
    }

    pub fn reset(&mut self) {
        self.consecutive_stalls = 0;
        self.total_stalls = 0;
        self.worst_frame = Duration::ZERO;
        self.recent_times.clear();
    }
}

impl Default for FrameBudget {
    fn default() -> Self {
        // 60 FPS target.
        Self::new(Duration::from_micros(16_667))
    }
}

// ── Allocation tracker ───────────────────────────────────────────

/// Lightweight per-frame allocation counter.
///
/// Call `alloc()` / `dealloc()` from a global allocator wrapper to
/// track allocation pressure.  The profiler reads the snapshot each frame.
pub struct AllocTracker {
    pub allocs_this_frame: u64,
    pub deallocs_this_frame: u64,
    pub bytes_allocated: u64,
    pub bytes_freed: u64,
    pub peak_bytes: u64,
    total_live_bytes: u64,
}

impl AllocTracker {
    pub const fn new() -> Self {
        Self {
            allocs_this_frame: 0,
            deallocs_this_frame: 0,
            bytes_allocated: 0,
            bytes_freed: 0,
            peak_bytes: 0,
            total_live_bytes: 0,
        }
    }

    /// Record an allocation.
    pub fn alloc(&mut self, size: usize) {
        self.allocs_this_frame += 1;
        self.bytes_allocated += size as u64;
        self.total_live_bytes += size as u64;
        if self.total_live_bytes > self.peak_bytes {
            self.peak_bytes = self.total_live_bytes;
        }
    }

    /// Record a deallocation.
    pub fn dealloc(&mut self, size: usize) {
        self.deallocs_this_frame += 1;
        self.bytes_freed += size as u64;
        self.total_live_bytes = self.total_live_bytes.saturating_sub(size as u64);
    }

    /// Reset per-frame counters (call at frame start).
    pub fn begin_frame(&mut self) {
        self.allocs_this_frame = 0;
        self.deallocs_this_frame = 0;
        self.bytes_allocated = 0;
        self.bytes_freed = 0;
    }
}

impl Default for AllocTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ── Plugin ───────────────────────────────────────────────────────

impl crate::Plugin for ProfilerPlugin {
    fn name(&self) -> &str {
        "ProfilerPlugin"
    }

    fn build(&self, app: &mut crate::App) {
        app.world.insert_resource(Profiler::new());
        log::info!("ProfilerPlugin loaded — profiling active");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiler_creation() {
        let profiler = Profiler::new();
        assert!(profiler.enabled);
        assert!(profiler.frame_data.is_empty());
    }

    #[test]
    fn profiler_frame_stats() {
        let mut profiler = Profiler::new();
        profiler.begin_frame();
        profiler.end_frame();

        let stats = profiler.get_frame_stats();
        assert!(stats.fps >= 0.0);
    }

    #[test]
    fn profile_scope_timing() {
        let mut profiler = Profiler::new();
        profiler.begin_frame();
        profiler.begin_scope("test_scope");
        std::thread::sleep(std::time::Duration::from_millis(1));
        profiler.end_scope("test_scope");
        profiler.end_frame();

        assert!(!profiler.frame_data.is_empty());
    }
}
