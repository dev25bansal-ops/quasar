use crate::stats::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChromeTraceEvent {
    pub name: String,
    #[serde(rename = "cat")]
    pub category: String,
    #[serde(rename = "ph")]
    pub phase: String,
    #[serde(rename = "ts")]
    pub timestamp: u64,
    #[serde(rename = "dur", skip_serializing_if = "Option::is_none")]
    pub duration: Option<u64>,
    #[serde(rename = "pid")]
    pub process_id: u64,
    #[serde(rename = "tid")]
    pub thread_id: u64,
    #[serde(rename = "args", skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
}

impl ChromeTraceEvent {
    pub fn begin(name: &str, category: &str, timestamp_ns: u64, thread_id: u64) -> Self {
        Self {
            name: name.to_string(),
            category: category.to_string(),
            phase: "B".to_string(),
            timestamp: timestamp_ns / 1000,
            duration: None,
            process_id: 0,
            thread_id,
            args: None,
        }
    }

    pub fn end(name: &str, category: &str, timestamp_ns: u64, thread_id: u64) -> Self {
        Self {
            name: name.to_string(),
            category: category.to_string(),
            phase: "E".to_string(),
            timestamp: timestamp_ns / 1000,
            duration: None,
            process_id: 0,
            thread_id,
            args: None,
        }
    }

    pub fn complete(
        name: &str,
        category: &str,
        start_ns: u64,
        duration_ns: u64,
        thread_id: u64,
    ) -> Self {
        Self {
            name: name.to_string(),
            category: category.to_string(),
            phase: "X".to_string(),
            timestamp: start_ns / 1000,
            duration: Some(duration_ns / 1000),
            process_id: 0,
            thread_id,
            args: None,
        }
    }

    pub fn metadata(name: &str, value: serde_json::Value) -> Self {
        Self {
            name: name.to_string(),
            category: "metadata".to_string(),
            phase: "M".to_string(),
            timestamp: 0,
            duration: None,
            process_id: 0,
            thread_id: 0,
            args: Some(value),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ChromeTraceExport {
    events: Vec<ChromeTraceEvent>,
    start_time_ns: Option<u64>,
}

impl ChromeTraceExport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_event(&mut self, event: ChromeTraceEvent) {
        if self.start_time_ns.is_none() || event.timestamp < self.start_time_ns.unwrap() {
            self.start_time_ns = Some(event.timestamp);
        }
        self.events.push(event);
    }

    pub fn add_timing(
        &mut self,
        name: &str,
        category: &str,
        start_ns: u64,
        end_ns: u64,
        thread_id: u64,
    ) {
        let event =
            ChromeTraceEvent::complete(name, category, start_ns, end_ns - start_ns, thread_id);
        self.add_event(event);
    }

    pub fn add_frame(&mut self, frame_number: u64, start_ns: u64, end_ns: u64, thread_id: u64) {
        self.add_timing(
            &format!("Frame {}", frame_number),
            "frame",
            start_ns,
            end_ns,
            thread_id,
        );
    }

    pub fn from_cpu_profiler(
        records: &[crate::TimingRecord],
        scopes: &HashMap<ScopeId, ScopeInfo>,
    ) -> Self {
        let mut export = Self::new();

        for record in records {
            if let Some(scope) = scopes.get(&record.scope_id) {
                export.add_timing(
                    &scope.name,
                    "cpu",
                    record.start_ns,
                    record.end_ns,
                    record.thread_id,
                );
            }
        }

        export
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.events).unwrap_or(serde_json::Value::Array(Vec::new()))
    }

    pub fn to_json_string(&self) -> String {
        serde_json::to_string(&self.events).unwrap_or_else(|_| "[]".to_string())
    }

    pub fn write_to_file(&self, path: &std::path::Path) -> std::io::Result<()> {
        let mut file = std::fs::File::create(path)?;
        file.write_all(self.to_json_string().as_bytes())?;
        Ok(())
    }

    pub fn events(&self) -> &[ChromeTraceEvent] {
        &self.events
    }

    pub fn clear(&mut self) {
        self.events.clear();
        self.start_time_ns = None;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilerReport {
    pub generated_at: String,
    pub frame_stats: Option<StatisticsJson>,
    pub scope_stats: HashMap<String, StatisticsJson>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticsJson {
    pub min_us: u64,
    pub max_us: u64,
    pub avg_us: u64,
    pub median_us: u64,
    pub p95_us: u64,
    pub p99_us: u64,
    pub count: usize,
}

impl From<&Statistics> for StatisticsJson {
    fn from(stats: &Statistics) -> Self {
        Self {
            min_us: stats.min.as_micros() as u64,
            max_us: stats.max.as_micros() as u64,
            avg_us: stats.avg.as_micros() as u64,
            median_us: stats.median.as_micros() as u64,
            p95_us: stats.p95.as_micros() as u64,
            p99_us: stats.p99.as_micros() as u64,
            count: stats.count,
        }
    }
}

impl ProfilerReport {
    pub fn new() -> Self {
        Self {
            generated_at: chrono_lite::now(),
            frame_stats: None,
            scope_stats: HashMap::new(),
        }
    }

    pub fn with_frame_stats(mut self, stats: &Statistics) -> Self {
        self.frame_stats = Some(StatisticsJson::from(stats));
        self
    }

    pub fn add_scope(&mut self, name: &str, stats: &Statistics) {
        self.scope_stats
            .insert(name.to_string(), StatisticsJson::from(stats));
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Object(serde_json::Map::new()))
    }

    pub fn write_to_file(&self, path: &std::path::Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

mod chrono_lite {
    pub fn now() -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        format!("{}", now.as_secs())
    }
}
