use quasar_ai::BlackboardValue;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub enum SimNodeStatus {
    Success,
    Failure,
    Running,
    Idle,
}

impl SimNodeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SimNodeStatus::Success => "Success",
            SimNodeStatus::Failure => "Failure",
            SimNodeStatus::Running => "Running",
            SimNodeStatus::Idle => "Idle",
        }
    }
}

pub struct SimulationState;

pub struct SimulationStats {
    pub tick_count: u64,
    pub current_status: SimNodeStatus,
    pub active_nodes: usize,
}

pub struct TraceEntry {
    pub tick: u64,
    pub node_name: String,
    pub status: String,
}

pub struct BtSimulation {
    _trace: Vec<TraceEntry>,
    _bb: HashMap<String, BlackboardValue>,
}

impl Default for BtSimulation {
    fn default() -> Self {
        Self::new()
    }
}

impl BtSimulation {
    pub fn new() -> Self {
        Self {
            _trace: Vec::new(),
            _bb: HashMap::new(),
        }
    }

    pub fn trace(&self) -> &[TraceEntry] {
        &self._trace
    }

    pub fn blackboard_snapshot(&self) -> &HashMap<String, BlackboardValue> {
        &self._bb
    }

    pub fn stats(&self) -> SimulationStats {
        SimulationStats {
            tick_count: 0,
            current_status: SimNodeStatus::Idle,
            active_nodes: 0,
        }
    }

    pub fn is_running(&self) -> bool {
        false
    }

    pub fn is_paused(&self) -> bool {
        false
    }

    pub fn pause(&mut self) {}
    pub fn stop(&mut self) {}
    pub fn start(&mut self) {}
    pub fn step(&mut self) {}

    pub fn load_from_graph<T>(&mut self, _graph: &T) {}
}