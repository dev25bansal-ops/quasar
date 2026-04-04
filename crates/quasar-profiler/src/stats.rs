use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::Duration;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Statistics {
    pub min: Duration,
    pub max: Duration,
    pub avg: Duration,
    pub median: Duration,
    pub p95: Duration,
    pub p99: Duration,
    pub count: usize,
}

impl Statistics {
    pub fn from_samples(samples: &[Duration]) -> Self {
        if samples.is_empty() {
            return Self {
                min: Duration::ZERO,
                max: Duration::ZERO,
                avg: Duration::ZERO,
                median: Duration::ZERO,
                p95: Duration::ZERO,
                p99: Duration::ZERO,
                count: 0,
            };
        }

        let mut sorted: Vec<_> = samples.iter().copied().collect();
        sorted.sort();

        let min = sorted.first().copied().unwrap();
        let max = sorted.last().copied().unwrap();
        let avg = sorted.iter().sum::<Duration>() / sorted.len() as u32;
        let median = percentile(&sorted, 50);
        let p95 = percentile(&sorted, 95);
        let p99 = percentile(&sorted, 99);

        Self {
            min,
            max,
            avg,
            median,
            p95,
            p99,
            count: samples.len(),
        }
    }
}

fn percentile(sorted: &[Duration], p: u32) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let idx = ((sorted.len() - 1) as f64 * (p as f64 / 100.0)).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

#[derive(Debug, Clone)]
pub struct RollingStats {
    samples: VecDeque<Duration>,
    max_samples: usize,
    cached_stats: Option<Statistics>,
    dirty: bool,
}

impl RollingStats {
    pub fn new(max_samples: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(max_samples),
            max_samples,
            cached_stats: None,
            dirty: true,
        }
    }

    pub fn add(&mut self, sample: Duration) {
        if self.samples.len() >= self.max_samples {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
        self.dirty = true;
    }

    pub fn statistics(&mut self) -> &Statistics {
        if self.dirty {
            self.cached_stats = Some(Statistics::from_samples(
                &self.samples.iter().copied().collect::<Vec<_>>(),
            ));
            self.dirty = false;
        }
        self.cached_stats.as_ref().unwrap()
    }

    pub fn clear(&mut self) {
        self.samples.clear();
        self.cached_stats = None;
        self.dirty = true;
    }

    pub fn samples(&self) -> impl Iterator<Item = &Duration> {
        self.samples.iter()
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

impl Default for RollingStats {
    fn default() -> Self {
        Self::new(1000)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ScopeId(pub u64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeInfo {
    pub id: ScopeId,
    pub name: String,
    pub parent: Option<ScopeId>,
    pub depth: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TimingRecord {
    pub scope_id: ScopeId,
    pub start_ns: u64,
    pub end_ns: u64,
    pub thread_id: u64,
    pub frame: u64,
}

impl TimingRecord {
    pub fn duration(&self) -> Duration {
        Duration::from_nanos(self.end_ns - self.start_ns)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FrameStats {
    pub frame_number: u64,
    pub frame_time: Duration,
    pub cpu_time: Duration,
    #[cfg(feature = "gpu")]
    pub gpu_time: Option<Duration>,
}
