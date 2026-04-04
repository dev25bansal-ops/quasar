use crate::stats::*;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

static SCOPE_COUNTER: AtomicU64 = AtomicU64::new(0);
static FRAME_COUNTER: AtomicU64 = AtomicU64::new(0);

fn get_thread_id() -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::thread::current().id().hash(&mut hasher);
    hasher.finish()
}

thread_local! {
    static CURRENT_FRAME: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static SCOPE_STACK: std::cell::RefCell<Vec<ScopeId>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[derive(Debug)]
pub struct CpuProfiler {
    scopes: RwLock<HashMap<ScopeId, ScopeInfo>>,
    scope_names: RwLock<HashMap<String, ScopeId>>,
    records: RwLock<Vec<TimingRecord>>,
    frame_times: RwLock<Vec<Duration>>,
    max_records: usize,
    max_frames: usize,
    enabled: std::sync::atomic::AtomicBool,
}

impl CpuProfiler {
    pub fn new() -> Self {
        Self {
            scopes: RwLock::new(HashMap::new()),
            scope_names: RwLock::new(HashMap::new()),
            records: RwLock::new(Vec::new()),
            frame_times: RwLock::new(Vec::new()),
            max_records: 100_000,
            max_frames: 1000,
            enabled: std::sync::atomic::AtomicBool::new(true),
        }
    }

    pub fn with_limits(max_records: usize, max_frames: usize) -> Self {
        Self {
            scopes: RwLock::new(HashMap::new()),
            scope_names: RwLock::new(HashMap::new()),
            records: RwLock::new(Vec::new()),
            frame_times: RwLock::new(Vec::new()),
            max_records,
            max_frames,
            enabled: std::sync::atomic::AtomicBool::new(true),
        }
    }

    pub fn register_scope(&self, name: &str) -> ScopeId {
        let mut names = self.scope_names.write();
        if let Some(&id) = names.get(name) {
            return id;
        }

        let id = ScopeId(SCOPE_COUNTER.fetch_add(1, Ordering::Relaxed));
        names.insert(name.to_string(), id);

        let parent = SCOPE_STACK.with(|s| s.borrow().last().copied());
        let depth = SCOPE_STACK.with(|s| s.borrow().len() as u32);

        let scope = ScopeInfo {
            id,
            name: name.to_string(),
            parent,
            depth,
        };

        self.scopes.write().insert(id, scope);
        id
    }

    pub fn begin_scope(&self, scope_id: ScopeId) -> Instant {
        SCOPE_STACK.with(|s| s.borrow_mut().push(scope_id));
        Instant::now()
    }

    pub fn end_scope(&self, scope_id: ScopeId, start: Instant) {
        SCOPE_STACK.with(|s| {
            let mut stack = s.borrow_mut();
            if let Some(last) = stack.pop() {
                debug_assert_eq!(last, scope_id);
            }
        });

        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }

        let end = Instant::now();
        let start_ns = start.elapsed().as_nanos() as u64;
        let end_ns = end.elapsed().as_nanos() as u64;
        let thread_id = get_thread_id();

        let frame = CURRENT_FRAME.with(|f| f.get());

        let record = TimingRecord {
            scope_id,
            start_ns,
            end_ns,
            thread_id,
            frame,
        };

        let mut records = self.records.write();
        if records.len() >= self.max_records {
            let half = records.len() / 2;
            records.drain(0..half);
        }
        records.push(record);
    }

    pub fn begin_frame(&self) -> u64 {
        let frame = FRAME_COUNTER.fetch_add(1, Ordering::Relaxed);
        CURRENT_FRAME.with(|f| f.set(frame));
        frame
    }

    pub fn end_frame(&self, frame_start: Instant) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }

        let frame_time = frame_start.elapsed();
        let mut times = self.frame_times.write();
        if times.len() >= self.max_frames {
            let half = times.len() / 2;
            times.drain(0..half);
        }
        times.push(frame_time);
    }

    pub fn frame_statistics(&self) -> Statistics {
        let times = self.frame_times.read();
        Statistics::from_samples(&times.iter().copied().collect::<Vec<_>>())
    }

    pub fn scope_statistics(&self, scope_id: ScopeId) -> Statistics {
        let records = self.records.read();
        let durations: Vec<_> = records
            .iter()
            .filter(|r| r.scope_id == scope_id)
            .map(|r| r.duration())
            .collect();
        Statistics::from_samples(&durations)
    }

    pub fn all_scope_statistics(&self) -> HashMap<ScopeId, Statistics> {
        let records = self.records.read();
        let mut by_scope: HashMap<ScopeId, Vec<Duration>> = HashMap::new();

        for record in records.iter() {
            by_scope
                .entry(record.scope_id)
                .or_default()
                .push(record.duration());
        }

        by_scope
            .into_iter()
            .map(|(id, durations)| (id, Statistics::from_samples(&durations)))
            .collect()
    }

    pub fn records(&self) -> Vec<TimingRecord> {
        self.records.read().clone()
    }

    pub fn scopes(&self) -> HashMap<ScopeId, ScopeInfo> {
        self.scopes.read().clone()
    }

    pub fn clear(&self) {
        self.records.write().clear();
        self.frame_times.write().clear();
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }
}

impl Default for CpuProfiler {
    fn default() -> Self {
        Self::new()
    }
}

use std::sync::LazyLock;

static GLOBAL_PROFILER: LazyLock<Arc<CpuProfiler>> = LazyLock::new(|| Arc::new(CpuProfiler::new()));

pub fn global() -> Arc<CpuProfiler> {
    GLOBAL_PROFILER.clone()
}

pub struct ScopedTimer {
    profiler: Arc<CpuProfiler>,
    scope_id: ScopeId,
    start: Instant,
}

impl ScopedTimer {
    pub fn new(name: &str) -> Self {
        let profiler = global();
        let scope_id = profiler.register_scope(name);
        let start = profiler.begin_scope(scope_id);
        Self {
            profiler,
            scope_id,
            start,
        }
    }

    pub fn with_profiler(profiler: Arc<CpuProfiler>, name: &str) -> Self {
        let scope_id = profiler.register_scope(name);
        let start = profiler.begin_scope(scope_id);
        Self {
            profiler,
            scope_id,
            start,
        }
    }
}

impl Drop for ScopedTimer {
    fn drop(&mut self) {
        self.profiler.end_scope(self.scope_id, self.start);
    }
}

#[macro_export]
macro_rules! profile_scope {
    ($name:expr) => {
        let _timer = $crate::ScopedTimer::new($name);
    };
    ($name:expr, $profiler:expr) => {
        let _timer = $crate::ScopedTimer::with_profiler($profiler, $name);
    };
}

#[macro_export]
macro_rules! profile_function {
    () => {
        let _timer = $crate::ScopedTimer::new(concat!(module_path!(), "::", function_name!()));
    };
}

pub struct FrameGuard {
    profiler: Arc<CpuProfiler>,
    start: Instant,
}

impl FrameGuard {
    pub fn new() -> Self {
        let profiler = global();
        profiler.begin_frame();
        Self {
            profiler,
            start: Instant::now(),
        }
    }

    pub fn with_profiler(profiler: Arc<CpuProfiler>) -> Self {
        profiler.begin_frame();
        Self {
            profiler,
            start: Instant::now(),
        }
    }
}

impl Drop for FrameGuard {
    fn drop(&mut self) {
        self.profiler.end_frame(self.start);
    }
}

impl Default for FrameGuard {
    fn default() -> Self {
        Self::new()
    }
}

pub fn function_stats<F: FnOnce() -> T, T>(name: &str, f: F) -> (T, Duration) {
    let timer = ScopedTimer::new(name);
    let result = f();
    let duration = timer.start.elapsed();
    drop(timer);
    (result, duration)
}
