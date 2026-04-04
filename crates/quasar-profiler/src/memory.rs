use parking_lot::RwLock;
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

static ALLOCATION_COUNT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AllocationId(pub u64);

#[derive(Debug, Clone)]
pub struct AllocationInfo {
    pub id: AllocationId,
    pub size: usize,
    pub layout: Layout,
    pub backtrace: Option<Vec<String>>,
    pub created_at: std::time::Instant,
}

#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub total_allocated: usize,
    pub total_deallocated: usize,
    pub current_usage: usize,
    pub allocation_count: u64,
    pub deallocation_count: u64,
    pub peak_usage: usize,
    pub live_allocations: usize,
}

impl MemoryStats {
    pub fn new() -> Self {
        Self {
            total_allocated: 0,
            total_deallocated: 0,
            current_usage: 0,
            allocation_count: 0,
            deallocation_count: 0,
            peak_usage: 0,
            live_allocations: 0,
        }
    }
}

impl Default for MemoryStats {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct MemoryTracker {
    stats: RwLock<MemoryStats>,
    allocations: RwLock<HashMap<AllocationId, AllocationInfo>>,
    capture_backtraces: AtomicU64,
    min_track_size: AtomicUsize,
}

impl MemoryTracker {
    pub fn new() -> Self {
        Self {
            stats: RwLock::new(MemoryStats::new()),
            allocations: RwLock::new(HashMap::new()),
            capture_backtraces: AtomicU64::new(0),
            min_track_size: AtomicUsize::new(0),
        }
    }

    pub fn set_capture_backtraces(&self, enabled: bool) {
        self.capture_backtraces
            .store(if enabled { 1 } else { 0 }, Ordering::Relaxed);
    }

    pub fn set_min_track_size(&self, size: usize) {
        self.min_track_size.store(size, Ordering::Relaxed);
    }

    pub fn record_allocation(&self, layout: Layout) -> AllocationId {
        let id = AllocationId(ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed));
        let size = layout.size();
        let min_size = self.min_track_size.load(Ordering::Relaxed);

        if size >= min_size {
            let backtrace = if self.capture_backtraces.load(Ordering::Relaxed) != 0 {
                Some(capture_backtrace())
            } else {
                None
            };

            let info = AllocationInfo {
                id,
                size,
                layout,
                backtrace,
                created_at: std::time::Instant::now(),
            };

            self.allocations.write().insert(id, info);

            let mut stats = self.stats.write();
            stats.total_allocated += size;
            stats.current_usage += size;
            stats.allocation_count += 1;
            stats.live_allocations += 1;
            if stats.current_usage > stats.peak_usage {
                stats.peak_usage = stats.current_usage;
            }
        }

        id
    }

    pub fn record_deallocation(&self, id: AllocationId) {
        if let Some(info) = self.allocations.write().remove(&id) {
            let mut stats = self.stats.write();
            stats.total_deallocated += info.size;
            stats.current_usage -= info.size;
            stats.deallocation_count += 1;
            stats.live_allocations -= 1;
        }
    }

    pub fn statistics(&self) -> MemoryStats {
        self.stats.read().clone()
    }

    pub fn live_allocations(&self) -> Vec<AllocationInfo> {
        self.allocations.read().values().cloned().collect()
    }

    pub fn potential_leaks(&self, min_age: std::time::Duration) -> Vec<AllocationInfo> {
        let now = std::time::Instant::now();
        self.allocations
            .read()
            .values()
            .filter(|a| now.duration_since(a.created_at) > min_age)
            .cloned()
            .collect()
    }

    pub fn clear(&self) {
        self.allocations.write().clear();
        *self.stats.write() = MemoryStats::new();
    }
}

impl Default for MemoryTracker {
    fn default() -> Self {
        Self::new()
    }
}

fn capture_backtrace() -> Vec<String> {
    format!("{:?}", std::backtrace::Backtrace::capture())
        .lines()
        .skip(3)
        .take(10)
        .map(|s| s.to_string())
        .collect()
}

#[derive(Debug)]
pub struct TrackingAllocator<A = System> {
    tracker: RwLock<Option<std::sync::Arc<MemoryTracker>>>,
    inner: A,
}

impl<A> TrackingAllocator<A> {
    pub fn new(inner: A) -> Self {
        Self {
            tracker: RwLock::new(None),
            inner,
        }
    }

    pub fn set_tracker(&self, tracker: std::sync::Arc<MemoryTracker>) {
        *self.tracker.write() = Some(tracker);
    }

    pub fn clear_tracker(&self) {
        *self.tracker.write() = None;
    }
}

unsafe impl<A: GlobalAlloc> GlobalAlloc for TrackingAllocator<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = self.inner.alloc(layout);
        if !ptr.is_null() {
            if let Some(tracker) = self.tracker.read().as_ref() {
                tracker.record_allocation(layout);
            }
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let _ = self.tracker.read().as_ref();
        self.inner.dealloc(ptr, layout);
    }
}

impl Default for TrackingAllocator<System> {
    fn default() -> Self {
        Self::new(System)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_tracker() {
        let tracker = MemoryTracker::new();
        tracker.set_min_track_size(0);

        let layout = Layout::new::<u8>();
        let id = tracker.record_allocation(layout);

        let stats = tracker.statistics();
        assert_eq!(stats.allocation_count, 1);
        assert_eq!(stats.live_allocations, 1);

        tracker.record_deallocation(id);

        let stats = tracker.statistics();
        assert_eq!(stats.deallocation_count, 1);
        assert_eq!(stats.live_allocations, 0);
    }
}
