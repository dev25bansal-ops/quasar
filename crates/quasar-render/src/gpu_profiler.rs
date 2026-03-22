//! GPU profiler — timestamp queries around render graph passes.
//!
//! Wraps every render graph pass with `wgpu` timestamp queries and resolves
//! them after submission, producing a `Vec<(String, f64)>` of pass names
//! and durations in milliseconds that the editor can display.

/// Maximum number of profiled passes per frame.
const MAX_PASSES: usize = 64;

/// GPU-side frame profiler using wgpu timestamp queries.
pub struct GpuProfiler {
    /// Timestamp query set (2 queries per pass: begin + end).
    query_set: wgpu::QuerySet,
    /// Buffer for resolved timestamp values.
    resolve_buf: wgpu::Buffer,
    /// Staging buffer for CPU read-back.
    staging_buf: wgpu::Buffer,
    /// Names of the passes profiled this frame, in order.
    pass_names: Vec<String>,
    /// Number of passes recorded this frame.
    pass_count: usize,
    /// Nanoseconds per GPU tick (from adapter timestamp period).
    timestamp_period: f32,
    /// Results from the previous frame (available after map completes).
    results: Vec<(String, f64)>,
    /// Whether a read-back is currently pending.
    pending: bool,
}

impl GpuProfiler {
    /// Create a new GPU profiler.
    ///
    /// `timestamp_period` is obtained from `adapter.get_info()` or
    /// `queue.get_timestamp_period()`.
    pub fn new(device: &wgpu::Device, timestamp_period: f32) -> Self {
        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("gpu_profiler_queries"),
            ty: wgpu::QueryType::Timestamp,
            count: (MAX_PASSES * 2) as u32,
        });

        let buf_size = (MAX_PASSES * 2 * std::mem::size_of::<u64>()) as u64;

        let resolve_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu_profiler_resolve"),
            size: buf_size,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu_profiler_staging"),
            size: buf_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            query_set,
            resolve_buf,
            staging_buf,
            pass_names: Vec::with_capacity(MAX_PASSES),
            pass_count: 0,
            timestamp_period,
            results: Vec::new(),
            pending: false,
        }
    }

    /// Call at the start of a new frame. Resets pass tracking.
    pub fn begin_frame(&mut self) {
        self.pass_names.clear();
        self.pass_count = 0;
    }

    /// Write a beginning timestamp for a named pass.
    /// Returns the query index pair `(begin, end)` the caller should use
    /// to write the end timestamp.
    pub fn begin_pass(&mut self, encoder: &mut wgpu::CommandEncoder, name: &str) -> Option<u32> {
        if self.pass_count >= MAX_PASSES {
            return None;
        }
        let begin_idx = (self.pass_count * 2) as u32;
        encoder.write_timestamp(&self.query_set, begin_idx);
        self.pass_names.push(name.to_string());
        Some(begin_idx)
    }

    /// Write the ending timestamp for a pass.
    pub fn end_pass(&mut self, encoder: &mut wgpu::CommandEncoder, begin_idx: u32) {
        let end_idx = begin_idx + 1;
        encoder.write_timestamp(&self.query_set, end_idx);
        self.pass_count += 1;
    }

    /// Resolve timestamps and copy to the staging buffer.
    /// Call after all passes are recorded, before `queue.submit`.
    pub fn resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        if self.pass_count == 0 {
            return;
        }
        let count = (self.pass_count * 2) as u32;
        encoder.resolve_query_set(&self.query_set, 0..count, &self.resolve_buf, 0);
        encoder.copy_buffer_to_buffer(
            &self.resolve_buf,
            0,
            &self.staging_buf,
            0,
            (self.pass_count * 2 * std::mem::size_of::<u64>()) as u64,
        );
    }

    /// Mark that results are pending. Call after `queue.submit`.
    pub fn request_results(&mut self) {
        if self.pass_count == 0 || self.pending {
            return;
        }
        self.pending = true;
    }

    /// Poll the device and try to harvest results.
    /// Returns the latest pass timings if a read-back completed.
    pub fn try_collect(&mut self, device: &wgpu::Device) -> Option<&[(String, f64)]> {
        if !self.pending {
            return if self.results.is_empty() {
                None
            } else {
                Some(&self.results)
            };
        }

        let (tx, rx) = std::sync::mpsc::channel();
        let slice = self.staging_buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        device.poll(wgpu::Maintain::Wait);

        match rx.recv() {
            Ok(Ok(())) => {}
            _ => {
                self.pending = false;
                return None;
            }
        }

        let slice = self.staging_buf.slice(..);
        let data = slice.get_mapped_range();
        let timestamps: &[u64] = bytemuck::cast_slice(&data);

        self.results.clear();
        for i in 0..self.pass_names.len() {
            let begin = timestamps[i * 2];
            let end = timestamps[i * 2 + 1];
            let duration_ns = (end.wrapping_sub(begin)) as f64 * self.timestamp_period as f64;
            let duration_ms = duration_ns / 1_000_000.0;
            self.results.push((self.pass_names[i].clone(), duration_ms));
        }

        drop(data);
        self.staging_buf.unmap();
        self.pending = false;

        Some(&self.results)
    }

    /// Get the last collected results without polling.
    pub fn last_results(&self) -> &[(String, f64)] {
        &self.results
    }
}
