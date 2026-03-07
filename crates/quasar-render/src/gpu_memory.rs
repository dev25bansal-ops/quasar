// crates/quasar-render/src/gpu_memory.rs
//! GPU memory budget tracking.
//!
//! Wraps `wgpu::Device` buffer / texture creation so every allocation is
//! book-kept. Provides:
//! - [`GpuMemoryTracker`]: global ledger of allocations.
//! - [`GpuAllocation`]: metadata for a single allocation.
//! - [`MemoryBudget`]: configurable budget with warning thresholds.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// Unique handle returned when an allocation is recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AllocationId(pub u64);

static NEXT_ALLOC_ID: AtomicU64 = AtomicU64::new(1);

fn next_alloc_id() -> AllocationId {
    AllocationId(NEXT_ALLOC_ID.fetch_add(1, Ordering::Relaxed))
}

/// The kind of GPU resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuResourceKind {
    Buffer,
    Texture,
    QuerySet,
}

/// Metadata for one tracked allocation.
#[derive(Debug, Clone)]
pub struct GpuAllocation {
    pub id: AllocationId,
    pub label: String,
    pub kind: GpuResourceKind,
    /// Size in bytes.
    pub size: u64,
    /// Optional category tag (e.g. "mesh", "lightmap", "shadow").
    pub category: String,
}

/// Budget + warning configuration.
#[derive(Debug, Clone)]
pub struct MemoryBudget {
    /// Hard budget in bytes (0 = unlimited).
    pub limit_bytes: u64,
    /// Fraction (0.0–1.0) at which a warning is emitted.
    pub warning_threshold: f32,
}

impl Default for MemoryBudget {
    fn default() -> Self {
        Self {
            // 1 GiB default
            limit_bytes: 1024 * 1024 * 1024,
            warning_threshold: 0.85,
        }
    }
}

/// Centralized GPU memory ledger.
#[derive(Debug)]
pub struct GpuMemoryTracker {
    allocations: HashMap<AllocationId, GpuAllocation>,
    total_bytes: u64,
    peak_bytes: u64,
    pub budget: MemoryBudget,
    /// Category totals (category → bytes).
    category_totals: HashMap<String, u64>,
    /// True once the warning threshold was exceeded (reset when memory drops back).
    warning_fired: bool,
}

impl Default for GpuMemoryTracker {
    fn default() -> Self {
        Self::new(MemoryBudget::default())
    }
}

impl GpuMemoryTracker {
    pub fn new(budget: MemoryBudget) -> Self {
        Self {
            allocations: HashMap::new(),
            total_bytes: 0,
            peak_bytes: 0,
            budget,
            category_totals: HashMap::new(),
            warning_fired: false,
        }
    }

    /// Record a new allocation, returns its tracking handle.
    pub fn record(
        &mut self,
        label: &str,
        kind: GpuResourceKind,
        size: u64,
        category: &str,
    ) -> AllocationId {
        let id = next_alloc_id();
        let alloc = GpuAllocation {
            id,
            label: label.to_string(),
            kind,
            size,
            category: category.to_string(),
        };
        self.allocations.insert(id, alloc);
        self.total_bytes += size;
        if self.total_bytes > self.peak_bytes {
            self.peak_bytes = self.total_bytes;
        }
        *self.category_totals.entry(category.to_string()).or_insert(0) += size;

        self.check_budget();
        id
    }

    /// Remove a previously-recorded allocation (e.g. when a buffer is dropped).
    pub fn release(&mut self, id: AllocationId) {
        if let Some(alloc) = self.allocations.remove(&id) {
            self.total_bytes = self.total_bytes.saturating_sub(alloc.size);
            if let Some(cat_total) = self.category_totals.get_mut(&alloc.category) {
                *cat_total = cat_total.saturating_sub(alloc.size);
            }
            if self.budget.limit_bytes > 0
                && (self.total_bytes as f32) < self.budget.limit_bytes as f32 * self.budget.warning_threshold
            {
                self.warning_fired = false;
            }
        }
    }

    fn check_budget(&mut self) {
        if self.budget.limit_bytes == 0 {
            return;
        }
        let ratio = self.total_bytes as f32 / self.budget.limit_bytes as f32;
        if ratio >= self.budget.warning_threshold && !self.warning_fired {
            log::warn!(
                "[GpuMemoryTracker] VRAM usage at {:.1}% ({} / {} bytes)",
                ratio * 100.0,
                self.total_bytes,
                self.budget.limit_bytes,
            );
            self.warning_fired = true;
        }
        if self.total_bytes > self.budget.limit_bytes {
            log::error!(
                "[GpuMemoryTracker] VRAM budget exceeded! {} / {} bytes",
                self.total_bytes,
                self.budget.limit_bytes,
            );
        }
    }

    // ── Public queries ───────────────────────────────────────────

    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    pub fn peak_bytes(&self) -> u64 {
        self.peak_bytes
    }

    pub fn allocation_count(&self) -> usize {
        self.allocations.len()
    }

    pub fn category_totals(&self) -> &HashMap<String, u64> {
        &self.category_totals
    }

    /// Sorted list of (label, kind, size) — heaviest first. Useful for the
    /// editor profiler panel.
    pub fn top_allocations(&self, max: usize) -> Vec<(&str, GpuResourceKind, u64)> {
        let mut sorted: Vec<_> = self
            .allocations
            .values()
            .map(|a| (a.label.as_str(), a.kind, a.size))
            .collect();
        sorted.sort_by(|a, b| b.2.cmp(&a.2));
        sorted.truncate(max);
        sorted
    }

    /// Human-readable summary (for debug overlay / console).
    pub fn summary(&self) -> String {
        let mb = |b: u64| b as f64 / (1024.0 * 1024.0);
        let mut s = format!(
            "GPU Memory: {:.1} MB / {:.1} MB   peak {:.1} MB   ({} allocs)\n",
            mb(self.total_bytes),
            mb(self.budget.limit_bytes),
            mb(self.peak_bytes),
            self.allocations.len(),
        );
        for (cat, bytes) in &self.category_totals {
            s.push_str(&format!("  {:<16} {:.2} MB\n", cat, mb(*bytes)));
        }
        s
    }

    /// Usage fraction 0.0–1.0 (for progress-bar in the editor).
    pub fn usage_fraction(&self) -> f32 {
        if self.budget.limit_bytes == 0 {
            return 0.0;
        }
        self.total_bytes as f32 / self.budget.limit_bytes as f32
    }

    /// Reset peak counter (e.g. at the start of a profiling session).
    pub fn reset_peak(&mut self) {
        self.peak_bytes = self.total_bytes;
    }
}

// ── Helper — byte-size of a wgpu texture ─────────────────────────
/// Rough byte count for texture memory (width × height × depth × bpp).
pub fn texture_byte_size(
    width: u32,
    height: u32,
    depth_or_array_layers: u32,
    format: wgpu::TextureFormat,
    mip_level_count: u32,
) -> u64 {
    let bpp: u64 = match format {
        wgpu::TextureFormat::R8Unorm
        | wgpu::TextureFormat::R8Snorm
        | wgpu::TextureFormat::R8Uint
        | wgpu::TextureFormat::R8Sint => 1,
        wgpu::TextureFormat::Rg8Unorm
        | wgpu::TextureFormat::Rg8Snorm
        | wgpu::TextureFormat::Rg8Uint
        | wgpu::TextureFormat::Rg8Sint => 2,
        wgpu::TextureFormat::Rgba8Unorm
        | wgpu::TextureFormat::Rgba8UnormSrgb
        | wgpu::TextureFormat::Rgba8Snorm
        | wgpu::TextureFormat::Rgba8Uint
        | wgpu::TextureFormat::Rgba8Sint
        | wgpu::TextureFormat::Bgra8Unorm
        | wgpu::TextureFormat::Bgra8UnormSrgb
        | wgpu::TextureFormat::Rgb10a2Unorm => 4,
        wgpu::TextureFormat::Rg16Float
        | wgpu::TextureFormat::Rg16Unorm
        | wgpu::TextureFormat::Rg16Snorm
        | wgpu::TextureFormat::Rg16Uint
        | wgpu::TextureFormat::Rg16Sint => 4,
        wgpu::TextureFormat::R16Float
        | wgpu::TextureFormat::R16Unorm
        | wgpu::TextureFormat::R16Snorm
        | wgpu::TextureFormat::R16Uint
        | wgpu::TextureFormat::R16Sint => 2,
        wgpu::TextureFormat::R32Float
        | wgpu::TextureFormat::R32Uint
        | wgpu::TextureFormat::R32Sint => 4,
        wgpu::TextureFormat::Rg32Float
        | wgpu::TextureFormat::Rg32Uint
        | wgpu::TextureFormat::Rg32Sint => 8,
        wgpu::TextureFormat::Rgba16Float
        | wgpu::TextureFormat::Rgba16Unorm
        | wgpu::TextureFormat::Rgba16Snorm
        | wgpu::TextureFormat::Rgba16Uint
        | wgpu::TextureFormat::Rgba16Sint => 8,
        wgpu::TextureFormat::Rgba32Float
        | wgpu::TextureFormat::Rgba32Uint
        | wgpu::TextureFormat::Rgba32Sint => 16,
        wgpu::TextureFormat::Depth32Float
        | wgpu::TextureFormat::Depth24Plus
        | wgpu::TextureFormat::Depth24PlusStencil8 => 4,
        wgpu::TextureFormat::Depth32FloatStencil8 => 5,
        _ => 4, // fallback
    };

    let mut total: u64 = 0;
    for mip in 0..mip_level_count {
        let w = (width >> mip).max(1) as u64;
        let h = (height >> mip).max(1) as u64;
        total += w * h * depth_or_array_layers as u64 * bpp;
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracker_basic() {
        let mut t = GpuMemoryTracker::new(MemoryBudget {
            limit_bytes: 1024,
            warning_threshold: 0.8,
        });
        let a = t.record("test_buf", GpuResourceKind::Buffer, 256, "mesh");
        assert_eq!(t.total_bytes(), 256);
        assert_eq!(t.allocation_count(), 1);
        t.release(a);
        assert_eq!(t.total_bytes(), 0);
    }

    #[test]
    fn tracker_peak() {
        let mut t = GpuMemoryTracker::default();
        let a = t.record("x", GpuResourceKind::Texture, 500, "shadow");
        let _b = t.record("y", GpuResourceKind::Buffer, 500, "mesh");
        assert_eq!(t.peak_bytes(), 1000);
        t.release(a);
        assert_eq!(t.peak_bytes(), 1000);
        assert_eq!(t.total_bytes(), 500);
    }
}
