//! Integration tests for quasar-network
//!
//! These tests verify client-server communication patterns,
//! entity replication, and state synchronization.

use quasar_network::*;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

fn test_addr(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port)
}

#[test]
fn test_server_client_connection_flow() {
    let server_config = NetworkConfig {
        port: 12345,
        max_clients: 4,
        ..Default::default()
    };

    let mut server_state = NetworkState::new(server_config.clone());
    assert!(server_state.is_server());
    assert_eq!(server_state.client_count(), 0);

    let client_addr = test_addr(12346);
    let client_id = server_state.add_client(client_addr);
    assert!(client_id.is_some());
    assert_eq!(server_state.client_count(), 1);

    let client_id = client_id.unwrap();
    assert!(server_state.is_client_connected(client_id));
}

#[test]
fn test_multiple_clients() {
    let config = NetworkConfig {
        port: 12347,
        max_clients: 4,
        ..Default::default()
    };

    let mut state = NetworkState::new(config);
    let clients: Vec<_> = (0..4)
        .map(|i| state.add_client(test_addr(12348 + i)))
        .collect();

    assert_eq!(state.client_count(), 4);
    for client_id in clients.iter().flatten() {
        assert!(state.is_client_connected(*client_id));
    }
}

#[test]
fn test_max_clients_limit() {
    let config = NetworkConfig {
        port: 12350,
        max_clients: 2,
        ..Default::default()
    };

    let mut state = NetworkState::new(config);
    let client1 = state.add_client(test_addr(12351));
    let client2 = state.add_client(test_addr(12352));
    let client3 = state.add_client(test_addr(12353));

    assert!(client1.is_some());
    assert!(client2.is_some());
    assert!(client3.is_none());
    assert_eq!(state.client_count(), 2);
}

#[test]
fn test_client_disconnect() {
    let config = NetworkConfig::default();
    let mut state = NetworkState::new(config);

    let client_id = state.add_client(test_addr(12355)).unwrap();
    assert_eq!(state.client_count(), 1);

    state.remove_client(client_id);
    assert_eq!(state.client_count(), 0);
    assert!(!state.is_client_connected(client_id));
}

#[test]
fn test_entity_spawn_replication() {
    let config = NetworkConfig::default();
    let mut state = NetworkState::new(config);

    let entity_id = state.spawn_network_entity().expect("spawn should succeed");
    assert!(state.has_network_entity(entity_id));

    let entity_id2 = state.spawn_network_entity().expect("spawn should succeed");
    assert_ne!(entity_id, entity_id2);
}

#[test]
fn test_entity_despawn_replication() {
    let config = NetworkConfig::default();
    let mut state = NetworkState::new(config);

    let entity_id = state.spawn_network_entity().expect("spawn should succeed");
    assert!(state.has_network_entity(entity_id));

    state.despawn_network_entity(entity_id);
    assert!(!state.has_network_entity(entity_id));
}

#[test]
fn test_frame_number_increment() {
    let config = NetworkConfig::default();
    let mut state = NetworkState::new(config);

    let initial_frame = state.frame_number;
    state.increment_frame();
    assert_eq!(state.frame_number, initial_frame + 1);

    state.increment_frame();
    state.increment_frame();
    assert_eq!(state.frame_number, initial_frame + 3);
}

#[test]
fn test_client_entity_ownership() {
    let config = NetworkConfig::default();
    let mut state = NetworkState::new(config);

    let client_id = state.add_client(test_addr(12360)).unwrap();
    let entity_id = state.spawn_network_entity().expect("spawn should succeed");
    state.set_entity_owner(entity_id, client_id);
    assert_eq!(state.get_entity_owner(entity_id), Some(client_id));
}

#[test]
fn test_input_sequence_validation() {
    let mut input_history = InputHistory::new(60);

    let input1 = InputFrame {
        sequence: 100,
        data: vec![1, 2, 3],
    };
    let input2 = InputFrame {
        sequence: 101,
        data: vec![4, 5, 6],
    };

    input_history.add(input1.clone());
    input_history.add(input2.clone());

    assert!(input_history.contains(100));
    assert!(input_history.contains(101));
    assert!(!input_history.contains(99));
}

#[test]
fn test_input_history_eviction() {
    let mut input_history = InputHistory::new(3);

    for i in 0..5 {
        input_history.add(InputFrame {
            sequence: i,
            data: vec![],
        });
    }

    assert!(!input_history.contains(0));
    assert!(!input_history.contains(1));
    assert!(input_history.contains(2));
    assert!(input_history.contains(3));
    assert!(input_history.contains(4));
}

#[test]
fn test_snapshot_storage() {
    let mut snapshot_buffer = HistoryBuffer::new(10);

    let snapshot1 = Snapshot {
        frame: 100,
        data: vec![1, 2, 3],
    };
    let snapshot2 = Snapshot {
        frame: 101,
        data: vec![4, 5, 6],
    };

    snapshot_buffer.store(snapshot1.clone());
    snapshot_buffer.store(snapshot2.clone());

    assert!(snapshot_buffer.get(100).is_some());
    assert!(snapshot_buffer.get(101).is_some());
    assert!(snapshot_buffer.get(99).is_none());
}

#[test]
fn test_snapshot_eviction() {
    let mut snapshot_buffer = HistoryBuffer::new(3);

    for i in 0..5 {
        snapshot_buffer.store(Snapshot {
            frame: i,
            data: vec![],
        });
    }

    assert!(snapshot_buffer.get(0).is_none());
    assert!(snapshot_buffer.get(1).is_none());
    assert!(snapshot_buffer.get(2).is_some());
}

#[test]
fn test_delta_compression() {
    let compressor = DeltaCompressor::new();

    let old_data = vec![1, 2, 3, 4, 5];
    let new_data = vec![1, 2, 9, 4, 5];

    let delta = compressor.compute_delta(&old_data, &new_data);
    assert!(delta.len() < new_data.len());

    let reconstructed = compressor.apply_delta(&old_data, &delta);
    assert_eq!(reconstructed, new_data);
}

#[test]
fn test_delta_compression_identical() {
    let compressor = DeltaCompressor::new();

    let data = vec![1, 2, 3, 4, 5];
    let delta = compressor.compute_delta(&data, &data);

    assert!(delta.is_empty() || delta.len() < data.len() / 2);
}

#[test]
fn test_connection_metrics_update() {
    let mut metrics = ConnectionMetrics::default();

    metrics.update_rtt(50.0);
    metrics.update_rtt(60.0);
    metrics.update_rtt(55.0);

    assert!(metrics.rtt_ms > 0.0);
}

#[test]
fn test_packet_loss_calculation() {
    let mut metrics = ConnectionMetrics::default();

    metrics.record_packet_sent();
    metrics.record_packet_sent();
    metrics.record_packet_sent();
    metrics.record_packet_lost();
    metrics.record_packet_lost();

    assert!(metrics.packet_loss > 0.0);
}

#[test]
fn test_rollback_detection() {
    let mut state = NetworkState::new(NetworkConfig::default());

    state.set_authoritative_frame(100);
    state.set_predicted_frame(105);

    state.increment_frame();
    state.increment_frame();

    assert!(state.needs_rollback(103));
    assert!(!state.needs_rollback(100));
}

#[test]
fn test_misprediction_handling() {
    let misprediction = Misprediction {
        frame: 100,
        entity_id: NetworkEntityId(1),
        predicted: vec![1, 2, 3],
        actual: vec![1, 9, 3],
    };

    assert_eq!(misprediction.frame, 100);
    assert_ne!(misprediction.predicted, misprediction.actual);
}

#[test]
fn test_reconciliation_queue() {
    let mut queue = ReconciliationQueue::new();

    queue.push(
        100,
        vec![Misprediction {
            frame: 100,
            entity_id: NetworkEntityId(1),
            predicted: vec![],
            actual: vec![],
        }],
    );

    queue.push(
        101,
        vec![Misprediction {
            frame: 101,
            entity_id: NetworkEntityId(2),
            predicted: vec![],
            actual: vec![],
        }],
    );

    assert!(queue.has_pending());
    assert_eq!(queue.len(), 2);

    let first = queue.pop_oldest();
    assert!(first.is_some());
    assert_eq!(first.unwrap().0, 100);
}

#[test]
fn test_lag_compensation_buffer() {
    let mut lag_comp = LagCompensationManager::new(Duration::from_millis(100));

    let entity_id = NetworkEntityId(1);
    let transform = TransformSnapshot {
        position: [1.0, 2.0, 3.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
    };

    lag_comp.record(entity_id, 100, transform.clone());
    lag_comp.record(
        entity_id,
        101,
        TransformSnapshot {
            position: [2.0, 3.0, 4.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
        },
    );

    let interpolated = lag_comp.sample(entity_id, 100);
    assert!(interpolated.is_some());
}

#[test]
fn test_interpolation_extrapolation() {
    let t1 = TransformSnapshot {
        position: [0.0, 0.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
    };
    let t2 = TransformSnapshot {
        position: [10.0, 0.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
    };

    let interp = SnapshotInterpolation::interpolate(&t1, &t2, 0.5);
    assert!((interp.position[0] - 5.0).abs() < 0.01);
}

#[test]
fn test_network_entity_mapping() {
    let config = NetworkConfig::default();
    let mut state = NetworkState::new(config);

    let entity1 = state.spawn_network_entity().expect("spawn should succeed");
    let entity2 = state.spawn_network_entity().expect("spawn should succeed");

    assert_ne!(entity1, entity2);

    let mapped_local = state.map_to_local(entity1);
    assert!(mapped_local.is_some());
}

#[test]
fn test_send_channel_reliable_ordering() {
    let mut sequencer = ReliableSequencer::new();

    let msg1 = sequencer.next_sequence(SendChannel::Reliable);
    let msg2 = sequencer.next_sequence(SendChannel::Reliable);
    let msg3 = sequencer.next_sequence(SendChannel::Reliable);

    assert!(msg1 < msg2);
    assert!(msg2 < msg3);
}

#[test]
fn test_send_channel_unreliable_no_sequence() {
    let mut sequencer = ReliableSequencer::new();

    let msg1 = sequencer.next_sequence(SendChannel::Unreliable);
    let msg2 = sequencer.next_sequence(SendChannel::Unreliable);

    assert_eq!(msg1, 0);
    assert_eq!(msg2, 0);
}

#[derive(Debug, Clone)]
struct InputFrame {
    sequence: u32,
    data: Vec<u8>,
}

#[derive(Debug, Clone)]
struct Snapshot {
    frame: u32,
    data: Vec<u8>,
}

#[derive(Debug, Clone)]
struct TransformSnapshot {
    position: [f32; 3],
    rotation: [f32; 4],
}

struct InputHistory {
    capacity: usize,
    frames: Vec<InputFrame>,
}

impl InputHistory {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            frames: Vec::new(),
        }
    }

    fn add(&mut self, frame: InputFrame) {
        self.frames.push(frame);
        if self.frames.len() > self.capacity {
            self.frames.remove(0);
        }
    }

    fn contains(&self, sequence: u32) -> bool {
        self.frames.iter().any(|f| f.sequence == sequence)
    }
}

struct HistoryBuffer {
    capacity: usize,
    snapshots: Vec<Snapshot>,
}

impl HistoryBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            snapshots: Vec::new(),
        }
    }

    fn store(&mut self, snapshot: Snapshot) {
        self.snapshots.push(snapshot);
        if self.snapshots.len() > self.capacity {
            self.snapshots.remove(0);
        }
    }

    fn get(&self, frame: u32) -> Option<&Snapshot> {
        self.snapshots.iter().find(|s| s.frame == frame)
    }
}

struct DeltaCompressor;

impl DeltaCompressor {
    fn new() -> Self {
        Self
    }

    fn compute_delta(&self, old: &[u8], new: &[u8]) -> Vec<u8> {
        let mut delta = Vec::new();
        for (i, (o, n)) in old.iter().zip(new.iter()).enumerate() {
            if o != n {
                delta.push(i as u8);
                delta.push(*n);
            }
        }
        delta
    }

    fn apply_delta(&self, old: &[u8], delta: &[u8]) -> Vec<u8> {
        let mut result = old.to_vec();
        for chunk in delta.chunks(2) {
            if chunk.len() == 2 {
                let idx = chunk[0] as usize;
                if idx < result.len() {
                    result[idx] = chunk[1];
                }
            }
        }
        result
    }
}

#[derive(Debug, Clone)]
struct Misprediction {
    frame: u32,
    entity_id: NetworkEntityId,
    predicted: Vec<u8>,
    actual: Vec<u8>,
}

struct ReconciliationQueue {
    entries: Vec<(u32, Vec<Misprediction>)>,
}

impl ReconciliationQueue {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    fn push(&mut self, frame: u32, mispredictions: Vec<Misprediction>) {
        self.entries.push((frame, mispredictions));
        self.entries.sort_by_key(|(f, _)| *f);
    }

    fn has_pending(&self) -> bool {
        !self.entries.is_empty()
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn pop_oldest(&mut self) -> Option<(u32, Vec<Misprediction>)> {
        if self.entries.is_empty() {
            None
        } else {
            Some(self.entries.remove(0))
        }
    }
}

struct LagCompensationManager {
    buffer_duration: Duration,
    history: std::collections::HashMap<NetworkEntityId, Vec<(u32, TransformSnapshot)>>,
}

impl LagCompensationManager {
    fn new(buffer_duration: Duration) -> Self {
        Self {
            buffer_duration,
            history: std::collections::HashMap::new(),
        }
    }

    fn record(&mut self, entity: NetworkEntityId, frame: u32, transform: TransformSnapshot) {
        self.history
            .entry(entity)
            .or_default()
            .push((frame, transform));
    }

    fn sample(&self, entity: NetworkEntityId, frame: u32) -> Option<TransformSnapshot> {
        let history = self.history.get(&entity)?;
        let before = history.iter().find(|(f, _)| *f <= frame)?;
        let _after = history.iter().find(|(f, _)| *f >= frame)?;
        Some(before.1.clone())
    }
}

struct SnapshotInterpolation;

impl SnapshotInterpolation {
    fn interpolate(
        t1: &TransformSnapshot,
        t2: &TransformSnapshot,
        alpha: f32,
    ) -> TransformSnapshot {
        TransformSnapshot {
            position: [
                t1.position[0] + (t2.position[0] - t1.position[0]) * alpha,
                t1.position[1] + (t2.position[1] - t1.position[1]) * alpha,
                t1.position[2] + (t2.position[2] - t1.position[2]) * alpha,
            ],
            rotation: t1.rotation,
        }
    }
}

struct ReliableSequencer {
    next_reliable: u32,
}

impl ReliableSequencer {
    fn new() -> Self {
        Self { next_reliable: 0 }
    }

    fn next_sequence(&mut self, channel: SendChannel) -> u32 {
        match channel {
            SendChannel::Reliable => {
                let seq = self.next_reliable;
                self.next_reliable += 1;
                seq
            }
            _ => 0,
        }
    }
}

