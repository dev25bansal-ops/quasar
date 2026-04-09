use crate::stats::*;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use wgpu::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GpuScopeId(pub u64);

#[derive(Debug, Clone)]
pub struct GpuScopeInfo {
    pub id: GpuScopeId,
    pub name: String,
}

pub struct GpuProfiler {
    device: Arc<Device>,
    queue: Arc<Queue>,
    query_type: QueryType,
    scopes: RwLock<HashMap<GpuScopeId, GpuScopeInfo>>,
    scope_names: RwLock<HashMap<String, GpuScopeId>>,
    pending_queries: RwLock<Vec<PendingQuery>>,
    resolved_times: RwLock<Vec<GpuTimingRecord>>,
    next_scope_id: AtomicU64,
}

struct PendingQuery {
    scope_id: GpuScopeId,
    query_buffer: Arc<Buffer>,
    resolve_buffer: Arc<Buffer>,
    start_query: u32,
    end_query: u32,
    frame: u64,
}

#[derive(Debug, Clone)]
pub struct GpuTimingRecord {
    pub scope_id: GpuScopeId,
    pub duration_ns: u64,
    pub frame: u64,
}

impl GpuProfiler {
    pub fn new(device: Arc<Device>, queue: Arc<Queue>) -> Self {
        Self {
            device,
            queue,
            query_type: QueryType::Timestamp,
            scopes: RwLock::new(HashMap::new()),
            scope_names: RwLock::new(HashMap::new()),
            pending_queries: RwLock::new(Vec::new()),
            resolved_times: RwLock::new(Vec::new()),
            next_scope_id: AtomicU64::new(0),
        }
    }

    pub fn register_scope(&self, name: &str) -> GpuScopeId {
        let mut names = self.scope_names.write();
        if let Some(&id) = names.get(name) {
            return id;
        }

        let id = GpuScopeId(self.next_scope_id.fetch_add(1, Ordering::Relaxed));
        names.insert(name.to_string(), id);

        let scope = GpuScopeInfo {
            id,
            name: name.to_string(),
        };

        self.scopes.write().insert(id, scope);
        id
    }

    pub fn begin_scope(
        &self,
        encoder: &mut CommandEncoder,
        scope_id: GpuScopeId,
    ) -> GpuScopeGuard<'_> {
        let query_set = self.create_query_set();
        encoder.write_timestamp(&query_set, 0);

        GpuScopeGuard {
            profiler: self,
            scope_id,
            encoder,
            started: true,
        }
    }

    pub fn end_scope(&self, encoder: &mut CommandEncoder, scope_id: GpuScopeId) {
        let query_set = self.create_query_set();
        encoder.write_timestamp(&query_set, 1);

        self.resolve_queries(encoder, scope_id);
    }

    fn create_query_set(&self) -> QuerySet {
        self.device.create_query_set(&QuerySetDescriptor {
            label: Some("profiler_timestamps"),
            count: 2,
            ty: QueryType::Timestamp,
        })
    }

    fn resolve_queries(&self, encoder: &mut CommandEncoder, scope_id: GpuScopeId) {
        let query_set = self.create_query_set();
        let resolve_buffer = self.device.create_buffer(&BufferDescriptor {
            label: Some("profiler_resolve"),
            size: 16,
            usage: BufferUsages::QUERY_RESOLVE | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let read_buffer = self.device.create_buffer(&BufferDescriptor {
            label: Some("profiler_read"),
            size: 16,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        encoder.resolve_query_set(&query_set, 0..2, &resolve_buffer, 0);
        encoder.copy_buffer_to_buffer(&resolve_buffer, 0, &read_buffer, 0, 16);
    }

    pub fn resolve_frame(&self) {
        // In a real implementation, you would map the buffers and read back the timestamps
        // This requires async operations and is simplified here
    }

    pub fn scope_statistics(&self, scope_id: GpuScopeId) -> Statistics {
        let times = self.resolved_times.read();
        let durations: Vec<_> = times
            .iter()
            .filter(|r| r.scope_id == scope_id)
            .map(|r| std::time::Duration::from_nanos(r.duration_ns))
            .collect();
        Statistics::from_samples(&durations)
    }

    pub fn all_scope_statistics(&self) -> HashMap<GpuScopeId, Statistics> {
        let times = self.resolved_times.read();
        let mut by_scope: HashMap<GpuScopeId, Vec<std::time::Duration>> = HashMap::new();

        for record in times.iter() {
            by_scope
                .entry(record.scope_id)
                .or_default()
                .push(std::time::Duration::from_nanos(record.duration_ns));
        }

        by_scope
            .into_iter()
            .map(|(id, durations)| (id, Statistics::from_samples(&durations)))
            .collect()
    }

    pub fn clear(&self) {
        self.pending_queries.write().clear();
        self.resolved_times.write().clear();
    }
}

use std::sync::atomic::{AtomicU64, Ordering};

pub struct GpuScopeGuard<'a> {
    profiler: &'a GpuProfiler,
    scope_id: GpuScopeId,
    encoder: &'a mut CommandEncoder,
    started: bool,
}

impl<'a> Drop for GpuScopeGuard<'a> {
    fn drop(&mut self) {
        if self.started {
            self.profiler.end_scope(self.encoder, self.scope_id);
        }
    }
}

#[macro_export]
macro_rules! profile_gpu_scope {
    ($profiler:expr, $encoder:expr, $name:expr) => {{
        let scope_id = $profiler.register_scope($name);
        $profiler.begin_scope($encoder, scope_id)
    }};
}

pub struct GpuFrameStats {
    pub frame_number: u64,
    pub gpu_time: std::time::Duration,
    pub scopes: HashMap<String, std::time::Duration>,
}

impl GpuFrameStats {
    pub fn new(frame_number: u64) -> Self {
        Self {
            frame_number,
            gpu_time: std::time::Duration::ZERO,
            scopes: HashMap::new(),
        }
    }
}
