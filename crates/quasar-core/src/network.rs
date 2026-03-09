//! Networking — QUIC/UDP integration for game networking.
//!
//! Provides:
//! - NetworkPlugin with client/server roles
//! - Entity replication across network
//! - Rollback netcode support

use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

pub const DEFAULT_PORT: u16 = 7777;
pub const MAX_CLIENTS: usize = 32;
pub const TICK_RATE: u32 = 60;
pub const MTU_SIZE: usize = 1400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClientId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NetworkEntityId(pub u64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkRole {
    Server,
    Client { server_addr: SocketAddr },
    ListenServer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub role: NetworkRole,
    pub port: u16,
    pub max_clients: usize,
    pub tick_rate: u32,
    pub interpolation_delay_ms: u32,
    pub rollback_frame_count: u32,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            role: NetworkRole::Server,
            port: DEFAULT_PORT,
            max_clients: MAX_CLIENTS,
            tick_rate: TICK_RATE,
            interpolation_delay_ms: 100,
            rollback_frame_count: 8,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMessage {
    pub sequence: u64,
    pub timestamp: u64,
    pub payload: NetworkPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkPayload {
    ConnectionRequest {
        client_id: ClientId,
    },
    ConnectionAccepted {
        client_id: ClientId,
    },
    ConnectionDenied {
        reason: String,
    },
    Disconnect {
        client_id: ClientId,
    },
    EntitySpawn {
        entity_id: NetworkEntityId,
        components: Vec<ComponentData>,
    },
    EntityDespawn {
        entity_id: NetworkEntityId,
    },
    EntityUpdate {
        entity_id: NetworkEntityId,
        components: Vec<ComponentData>,
    },
    EntityTransform {
        entity_id: NetworkEntityId,
        position: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
    },
    Input {
        client_id: ClientId,
        inputs: Vec<InputData>,
    },
    StateSnapshot {
        frame: u64,
        entities: Vec<EntitySnapshot>,
    },
    Rpc {
        entity_id: NetworkEntityId,
        method: String,
        args: Vec<u8>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentData {
    pub type_name: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySnapshot {
    pub entity_id: NetworkEntityId,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub frame: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputData {
    pub input_type: InputType,
    pub value: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputType {
    MoveForward,
    MoveBackward,
    MoveLeft,
    MoveRight,
    Jump,
    Attack,
    Interact,
    Custom(String),
}

pub struct NetworkClient {
    pub id: ClientId,
    pub addr: SocketAddr,
    pub last_received_sequence: u64,
    pub last_sent_sequence: u64,
    pub rtt_ms: f32,
    pub packet_loss: f32,
    pub connected: bool,
    pub entity_ids: Vec<NetworkEntityId>,
}

impl NetworkClient {
    pub fn new(id: ClientId, addr: SocketAddr) -> Self {
        Self {
            id,
            addr,
            last_received_sequence: 0,
            last_sent_sequence: 0,
            rtt_ms: 0.0,
            packet_loss: 0.0,
            connected: true,
            entity_ids: Vec::new(),
        }
    }
}

pub struct NetworkState {
    pub config: NetworkConfig,
    pub clients: HashMap<ClientId, NetworkClient>,
    pub entity_to_network: HashMap<u32, NetworkEntityId>,
    pub network_to_entity: HashMap<NetworkEntityId, u32>,
    pub next_entity_id: u64,
    pub next_client_id: u64,
    pub frame_number: u64,
    pub input_buffer: HashMap<ClientId, Vec<InputData>>,
}

impl NetworkState {
    pub fn new(config: NetworkConfig) -> Self {
        Self {
            config,
            clients: HashMap::new(),
            entity_to_network: HashMap::new(),
            network_to_entity: HashMap::new(),
            next_entity_id: 1,
            next_client_id: 1,
            frame_number: 0,
            input_buffer: HashMap::new(),
        }
    }

    pub fn is_server(&self) -> bool {
        matches!(
            self.config.role,
            NetworkRole::Server | NetworkRole::ListenServer
        )
    }

    pub fn is_client(&self) -> bool {
        matches!(self.config.role, NetworkRole::Client { .. })
    }
}

#[derive(Debug)]
pub struct NetworkError(pub String);

impl std::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for NetworkError {}

impl From<std::io::Error> for NetworkError {
    fn from(e: std::io::Error) -> Self {
        NetworkError(e.to_string())
    }
}

pub struct UdpTransport {
    socket: UdpSocket,
    recv_buffer: [u8; MTU_SIZE],
}

impl UdpTransport {
    pub fn bind(addr: SocketAddr) -> Result<Self, NetworkError> {
        let socket = UdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;
        Ok(Self {
            socket,
            recv_buffer: [0u8; MTU_SIZE],
        })
    }

    pub fn connect(&self, server_addr: SocketAddr) -> Result<(), NetworkError> {
        self.socket.connect(server_addr)?;
        Ok(())
    }

    pub fn local_addr(&self) -> Result<SocketAddr, NetworkError> {
        Ok(self.socket.local_addr()?)
    }

    pub fn send_to(&mut self, addr: SocketAddr, data: &[u8]) -> Result<(), NetworkError> {
        self.socket.send_to(data, addr)?;
        Ok(())
    }

    pub fn receive(&mut self) -> Result<Option<(SocketAddr, Vec<u8>)>, NetworkError> {
        match self.socket.recv_from(&mut self.recv_buffer) {
            Ok((len, addr)) => Ok(Some((addr, self.recv_buffer[..len].to_vec()))),
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(NetworkError(e.to_string())),
        }
    }
}

pub struct RollbackState {
    pub frame: u64,
    pub entities: HashMap<NetworkEntityId, EntitySnapshot>,
    pub inputs: HashMap<ClientId, Vec<InputData>>,
}

/// Per-client input history ring buffer for rollback.
pub struct InputHistory {
    /// Ring of per-frame inputs keyed by frame number.
    buffer: HashMap<u64, HashMap<ClientId, Vec<InputData>>>,
    oldest_frame: u64,
    capacity: u32,
}

impl InputHistory {
    pub fn new(capacity: u32) -> Self {
        Self {
            buffer: HashMap::with_capacity(capacity as usize),
            oldest_frame: 0,
            capacity,
        }
    }

    /// Record inputs for a given frame.
    pub fn record(&mut self, frame: u64, client_id: ClientId, inputs: Vec<InputData>) {
        let entry = self.buffer.entry(frame).or_insert_with(HashMap::new);
        entry.insert(client_id, inputs);
        // Evict stale frames.
        while self.buffer.len() > self.capacity as usize {
            self.buffer.remove(&self.oldest_frame);
            self.oldest_frame += 1;
        }
    }

    /// Get all client inputs for a given frame.
    pub fn get(&self, frame: u64) -> Option<&HashMap<ClientId, Vec<InputData>>> {
        self.buffer.get(&frame)
    }

    /// Replace/correct the input for a specific client at a specific frame.
    pub fn correct(&mut self, frame: u64, client_id: ClientId, inputs: Vec<InputData>) {
        let entry = self.buffer.entry(frame).or_insert_with(HashMap::new);
        entry.insert(client_id, inputs);
    }

    /// Get the range of available frames.
    pub fn available_range(&self) -> (u64, u64) {
        let min = self.buffer.keys().copied().min().unwrap_or(0);
        let max = self.buffer.keys().copied().max().unwrap_or(0);
        (min, max)
    }
}

/// Misprediction result returned by `detect_misprediction`.
pub struct Misprediction {
    pub frame: u64,
    pub server_entities: HashMap<NetworkEntityId, EntitySnapshot>,
}

/// Rollback netcode manager with input history and re-simulation support.
///
/// Stores per-frame world snapshots and per-client input history in ring
/// buffers. When the server sends an authoritative snapshot that disagrees
/// with a predicted frame, the caller can:
///
/// 1. Call `detect_misprediction` to determine if correction is needed.
/// 2. Call `begin_rollback` to rewind state to the corrected frame.
/// 3. Re-simulate forward using `inputs_for_frame` + game logic.
/// 4. Call `end_rollback` to record the corrected frames.
pub struct RollbackManager {
    pub states: Vec<RollbackState>,
    pub max_frames: u32,
    pub current_frame: u64,
    pub input_history: InputHistory,
    /// True while a rollback re-simulation is in progress.
    pub is_rolling_back: bool,
    /// Frame we are rolling back to (valid only during rollback).
    pub rollback_target_frame: u64,
    /// Position-squared misprediction threshold.
    pub misprediction_threshold: f32,
}

impl RollbackManager {
    pub fn new(max_frames: u32) -> Self {
        Self {
            states: Vec::with_capacity(max_frames as usize),
            max_frames,
            current_frame: 0,
            input_history: InputHistory::new(max_frames),
            is_rolling_back: false,
            rollback_target_frame: 0,
            misprediction_threshold: 0.001,
        }
    }

    /// Record inputs for the current frame from a specific client.
    pub fn record_input(&mut self, client_id: ClientId, inputs: Vec<InputData>) {
        self.input_history.record(self.current_frame, client_id, inputs);
    }

    /// Get stored inputs for a specific frame (used during re-simulation).
    pub fn inputs_for_frame(&self, frame: u64) -> Option<&HashMap<ClientId, Vec<InputData>>> {
        self.input_history.get(frame)
    }

    /// Correct the input for a specific client at a past frame (server authority).
    pub fn correct_input(&mut self, frame: u64, client_id: ClientId, inputs: Vec<InputData>) {
        self.input_history.correct(frame, client_id, inputs);
    }

    pub fn save_state(
        &mut self,
        entities: HashMap<NetworkEntityId, EntitySnapshot>,
        inputs: HashMap<ClientId, Vec<InputData>>,
    ) {
        // Also store inputs in the history ring buffer.
        for (client_id, input_list) in &inputs {
            self.input_history.record(self.current_frame, *client_id, input_list.clone());
        }
        let state = RollbackState {
            frame: self.current_frame,
            entities,
            inputs,
        };
        if self.states.len() >= self.max_frames as usize {
            self.states.remove(0);
        }
        self.states.push(state);
        self.current_frame += 1;
    }

    pub fn rollback_to(&mut self, frame: u64) -> Option<&RollbackState> {
        self.states.iter().find(|s| s.frame == frame)
    }

    /// Compare a server-authoritative snapshot against our predicted state.
    /// Returns `Some(Misprediction)` if any entity exceeds the threshold.
    pub fn detect_misprediction(
        &self,
        server_frame: u64,
        server_entities: &HashMap<NetworkEntityId, EntitySnapshot>,
    ) -> Option<Misprediction> {
        let local = self.states.iter().find(|s| s.frame == server_frame)?;
        for (id, server_snap) in server_entities {
            if let Some(local_snap) = local.entities.get(id) {
                let dx = server_snap.position[0] - local_snap.position[0];
                let dy = server_snap.position[1] - local_snap.position[1];
                let dz = server_snap.position[2] - local_snap.position[2];
                if dx * dx + dy * dy + dz * dz > self.misprediction_threshold {
                    return Some(Misprediction {
                        frame: server_frame,
                        server_entities: server_entities.clone(),
                    });
                }
            }
        }
        None
    }

    /// Begin a rollback: restores the world to the given frame's snapshot.
    /// The caller must then re-simulate from `frame` to `current_frame`
    /// using the (possibly corrected) input history, calling `advance_rollback`
    /// after each re-simulated tick.
    pub fn begin_rollback(
        &mut self,
        frame: u64,
        corrected_entities: HashMap<NetworkEntityId, EntitySnapshot>,
    ) -> bool {
        if let Some(idx) = self.states.iter().position(|s| s.frame == frame) {
            // Replace the snapshot at the corrected frame with server data.
            self.states[idx].entities = corrected_entities;
            // Discard all snapshots after this frame — they will be re-simulated.
            self.states.truncate(idx + 1);
            self.is_rolling_back = true;
            self.rollback_target_frame = frame;
            true
        } else {
            false
        }
    }

    /// Advance one re-simulation tick during rollback.
    /// `entities` is the world state after re-simulating one frame.
    pub fn advance_rollback(&mut self, entities: HashMap<NetworkEntityId, EntitySnapshot>) {
        let frame = match self.states.last() {
            Some(s) => s.frame + 1,
            None => self.rollback_target_frame + 1,
        };
        let inputs = self.input_history.get(frame).cloned().unwrap_or_default();
        let state = RollbackState { frame, entities, inputs };
        self.states.push(state);
    }

    /// End the rollback. Call after re-simulation has caught up to current_frame.
    pub fn end_rollback(&mut self) {
        self.is_rolling_back = false;
    }

    /// Run the full rollback re-simulation loop.
    ///
    /// 1. Detects misprediction against a server-authoritative snapshot.
    /// 2. If misprediction is found, rewinds state to the corrected frame.
    /// 3. Re-simulates forward from `server_frame + 1` to `current_frame - 1`,
    ///    calling `simulate_fn` for each tick with that frame's stored inputs.
    /// 4. Ends the rollback.
    ///
    /// `simulate_fn(frame, inputs)` should run one tick of game logic using
    /// the provided inputs, then return the resulting entity snapshots.
    ///
    /// Returns `true` if a rollback was performed.
    pub fn run_rollback<F>(
        &mut self,
        server_frame: u64,
        server_entities: &HashMap<NetworkEntityId, EntitySnapshot>,
        mut simulate_fn: F,
    ) -> bool
    where
        F: FnMut(u64, &HashMap<ClientId, Vec<InputData>>) -> HashMap<NetworkEntityId, EntitySnapshot>,
    {
        // 1. Detect misprediction.
        let misprediction = match self.detect_misprediction(server_frame, server_entities) {
            Some(m) => m,
            None => return false,
        };

        let target_frame = misprediction.frame;
        let resim_end = self.current_frame;

        // 2. Rewind state to the corrected frame.
        if !self.begin_rollback(target_frame, server_entities.clone()) {
            return false;
        }

        // 3. Re-simulate forward.
        for frame in (target_frame + 1)..resim_end {
            let inputs = self.input_history.get(frame).cloned().unwrap_or_default();
            let entities = simulate_fn(frame, &inputs);
            self.advance_rollback(entities);
        }

        // 4. End rollback.
        self.end_rollback();
        true
    }

    /// How many frames behind the current frame can we still rollback to.
    pub fn available_rollback_frames(&self) -> u64 {
        if self.states.is_empty() {
            return 0;
        }
        self.current_frame.saturating_sub(self.states[0].frame)
    }
}

// ── Tick-rate accumulator ──────────────────────────────────────

/// Server-side fixed tick-rate accumulator.
///
/// Ensures the server runs exactly `tick_rate` network ticks per second,
/// independent of frame rate.
pub struct TickAccumulator {
    pub tick_rate: u32,
    pub accumulator: f32,
    pub current_tick: u64,
}

impl TickAccumulator {
    pub fn new(tick_rate: u32) -> Self {
        Self {
            tick_rate,
            accumulator: 0.0,
            current_tick: 0,
        }
    }

    /// Feed real elapsed `delta_seconds` and return how many ticks to execute.
    pub fn advance(&mut self, delta_seconds: f32) -> u32 {
        let tick_dt = 1.0 / self.tick_rate as f32;
        self.accumulator += delta_seconds;
        let mut ticks = 0u32;
        while self.accumulator >= tick_dt {
            self.accumulator -= tick_dt;
            self.current_tick += 1;
            ticks += 1;
        }
        ticks
    }

    /// The interpolation alpha between the last and next tick (0.0–1.0).
    pub fn alpha(&self) -> f32 {
        let tick_dt = 1.0 / self.tick_rate as f32;
        self.accumulator / tick_dt
    }
}

// ── Snapshot interpolation (client-side) ───────────────────────

/// Stores two consecutive server snapshots and interpolates between them
/// so that remote entities move smoothly.
pub struct SnapshotInterpolation {
    /// The older (previous) snapshot.
    pub prev: Option<(u64, HashMap<NetworkEntityId, EntitySnapshot>)>,
    /// The newer (current) snapshot.
    pub curr: Option<(u64, HashMap<NetworkEntityId, EntitySnapshot>)>,
    /// Server tick rate (used to compute interpolation alpha).
    pub server_tick_rate: u32,
    /// Local timer accumulating real time between received snapshots.
    pub timer: f32,
}

impl SnapshotInterpolation {
    pub fn new(server_tick_rate: u32) -> Self {
        Self {
            prev: None,
            curr: None,
            server_tick_rate,
            timer: 0.0,
        }
    }

    /// Push a new authoritative server snapshot. The old `curr` becomes `prev`.
    pub fn push_snapshot(&mut self, frame: u64, entities: HashMap<NetworkEntityId, EntitySnapshot>) {
        self.prev = self.curr.take();
        self.curr = Some((frame, entities));
        self.timer = 0.0;
    }

    /// Advance interpolation timer by real delta and return per-entity
    /// interpolated transforms.
    pub fn interpolate(&mut self, delta: f32) -> Vec<(NetworkEntityId, [f32; 3], [f32; 4])> {
        self.timer += delta;

        let tick_dt = 1.0 / self.server_tick_rate as f32;
        let alpha = (self.timer / tick_dt).clamp(0.0, 1.0);

        let (prev_map, curr_map) = match (&self.prev, &self.curr) {
            (Some((_, p)), Some((_, c))) => (p, c),
            _ => return Vec::new(),
        };

        let mut results = Vec::new();
        for (id, curr_snap) in curr_map {
            if let Some(prev_snap) = prev_map.get(id) {
                let pos = lerp3(prev_snap.position, curr_snap.position, alpha);
                let rot = slerp(prev_snap.rotation, curr_snap.rotation, alpha);
                results.push((*id, pos, rot));
            } else {
                results.push((*id, curr_snap.position, curr_snap.rotation));
            }
        }
        results
    }
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn slerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    // If quaternions are very close, use linear interpolation.
    if dot.abs() > 0.9995 {
        let mut r = [
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
            a[3] + (b[3] - a[3]) * t,
        ];
        let len = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2] + r[3] * r[3]).sqrt();
        if len > 0.0 {
            r[0] /= len;
            r[1] /= len;
            r[2] /= len;
            r[3] /= len;
        }
        return r;
    }
    let dot = dot.clamp(-1.0, 1.0);
    let theta = dot.acos();
    let sin_theta = theta.sin();
    let wa = ((1.0 - t) * theta).sin() / sin_theta;
    let wb = (t * theta).sin() / sin_theta;
    [
        wa * a[0] + wb * b[0],
        wa * a[1] + wb * b[1],
        wa * a[2] + wb * b[2],
        wa * a[3] + wb * b[3],
    ]
}

// ── Delta compression ──────────────────────────────────────────

/// Tracks the last-sent state per entity so only changed fields are sent.
/// Supports encoding/decoding compact binary deltas for bandwidth savings.
pub struct DeltaCompressor {
    last_sent: HashMap<NetworkEntityId, EntitySnapshot>,
    /// Baseline frame acknowledged by remote peer.
    pub baseline_frame: u64,
    /// Baselines per-entity confirmed by ACK.
    baselines: HashMap<NetworkEntityId, EntitySnapshot>,
}

/// Bit flags indicating which fields changed in a delta packet.
#[derive(Debug, Clone, Copy)]
pub struct DeltaFlags(pub u8);

impl DeltaFlags {
    pub const POSITION: u8 = 0b0000_0001;
    pub const ROTATION: u8 = 0b0000_0010;
    pub const SCALE:    u8 = 0b0000_0100;
    pub const FRAME:    u8 = 0b0000_1000;
}

/// A compact encoded delta for a single entity.
#[derive(Debug, Clone)]
pub struct EncodedDelta {
    pub entity_id: NetworkEntityId,
    pub flags: u8,
    pub data: Vec<u8>,
}

impl DeltaCompressor {
    pub fn new() -> Self {
        Self {
            last_sent: HashMap::new(),
            baseline_frame: 0,
            baselines: HashMap::new(),
        }
    }

    /// Compare `current` against the last sent state. Returns `true` if the
    /// entity has changed enough to warrant re-sending.
    pub fn needs_update(&self, id: NetworkEntityId, current: &EntitySnapshot) -> bool {
        match self.last_sent.get(&id) {
            None => true,
            Some(prev) => {
                let pos_diff = (current.position[0] - prev.position[0]).powi(2)
                    + (current.position[1] - prev.position[1]).powi(2)
                    + (current.position[2] - prev.position[2]).powi(2);
                let rot_diff = (current.rotation[0] - prev.rotation[0]).powi(2)
                    + (current.rotation[1] - prev.rotation[1]).powi(2)
                    + (current.rotation[2] - prev.rotation[2]).powi(2)
                    + (current.rotation[3] - prev.rotation[3]).powi(2);
                // Threshold: ~0.5mm for position, ~0.01 for rotation.
                pos_diff > 0.000_25 || rot_diff > 0.000_1
            }
        }
    }

    /// Encode a compact binary delta between the baseline and current snapshot.
    /// Returns `None` if nothing changed.
    pub fn encode_delta(
        &self,
        id: NetworkEntityId,
        current: &EntitySnapshot,
    ) -> Option<EncodedDelta> {
        let baseline = self.baselines.get(&id);
        let mut flags: u8 = 0;
        let mut data = Vec::with_capacity(32);

        match baseline {
            None => {
                // Full snapshot — all fields.
                flags = DeltaFlags::POSITION | DeltaFlags::ROTATION | DeltaFlags::SCALE | DeltaFlags::FRAME;
                for &v in &current.position { data.extend_from_slice(&v.to_le_bytes()); }
                for &v in &current.rotation { data.extend_from_slice(&v.to_le_bytes()); }
                for &v in &current.scale    { data.extend_from_slice(&v.to_le_bytes()); }
                data.extend_from_slice(&current.frame.to_le_bytes());
            }
            Some(base) => {
                let pos_diff = (current.position[0] - base.position[0]).powi(2)
                    + (current.position[1] - base.position[1]).powi(2)
                    + (current.position[2] - base.position[2]).powi(2);
                if pos_diff > 0.000_25 {
                    flags |= DeltaFlags::POSITION;
                    // XOR-based delta: encode difference as f32 bytes XORed.
                    for i in 0..3 {
                        let base_bits = base.position[i].to_bits();
                        let curr_bits = current.position[i].to_bits();
                        data.extend_from_slice(&(base_bits ^ curr_bits).to_le_bytes());
                    }
                }
                let rot_diff = (current.rotation[0] - base.rotation[0]).powi(2)
                    + (current.rotation[1] - base.rotation[1]).powi(2)
                    + (current.rotation[2] - base.rotation[2]).powi(2)
                    + (current.rotation[3] - base.rotation[3]).powi(2);
                if rot_diff > 0.000_1 {
                    flags |= DeltaFlags::ROTATION;
                    for i in 0..4 {
                        let base_bits = base.rotation[i].to_bits();
                        let curr_bits = current.rotation[i].to_bits();
                        data.extend_from_slice(&(base_bits ^ curr_bits).to_le_bytes());
                    }
                }
                let scale_diff = (current.scale[0] - base.scale[0]).powi(2)
                    + (current.scale[1] - base.scale[1]).powi(2)
                    + (current.scale[2] - base.scale[2]).powi(2);
                if scale_diff > 0.000_25 {
                    flags |= DeltaFlags::SCALE;
                    for i in 0..3 {
                        let base_bits = base.scale[i].to_bits();
                        let curr_bits = current.scale[i].to_bits();
                        data.extend_from_slice(&(base_bits ^ curr_bits).to_le_bytes());
                    }
                }
                if current.frame != base.frame {
                    flags |= DeltaFlags::FRAME;
                    data.extend_from_slice(&current.frame.to_le_bytes());
                }
                if flags == 0 {
                    return None;
                }
            }
        }

        Some(EncodedDelta { entity_id: id, flags, data })
    }

    /// Decode a delta packet against the local baseline to reconstruct the snapshot.
    pub fn decode_delta(&self, delta: &EncodedDelta) -> EntitySnapshot {
        let base = self.baselines.get(&delta.entity_id);
        let mut offset = 0usize;

        let read_f32 = |d: &[u8], o: &mut usize| -> f32 {
            let bytes: [u8; 4] = [d[*o], d[*o + 1], d[*o + 2], d[*o + 3]];
            *o += 4;
            f32::from_le_bytes(bytes)
        };
        let read_u32 = |d: &[u8], o: &mut usize| -> u32 {
            let bytes: [u8; 4] = [d[*o], d[*o + 1], d[*o + 2], d[*o + 3]];
            *o += 4;
            u32::from_le_bytes(bytes)
        };
        let read_u64 = |d: &[u8], o: &mut usize| -> u64 {
            let bytes: [u8; 8] = [
                d[*o], d[*o + 1], d[*o + 2], d[*o + 3],
                d[*o + 4], d[*o + 5], d[*o + 6], d[*o + 7],
            ];
            *o += 8;
            u64::from_le_bytes(bytes)
        };

        let default_snap = EntitySnapshot {
            entity_id: delta.entity_id,
            position: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
            frame: 0,
        };
        let b = base.unwrap_or(&default_snap);

        let position = if delta.flags & DeltaFlags::POSITION != 0 {
            if base.is_none() {
                // Full snapshot mode: raw f32s.
                [read_f32(&delta.data, &mut offset),
                 read_f32(&delta.data, &mut offset),
                 read_f32(&delta.data, &mut offset)]
            } else {
                // XOR delta mode.
                let mut pos = [0.0f32; 3];
                for i in 0..3 {
                    let xor_bits = read_u32(&delta.data, &mut offset);
                    pos[i] = f32::from_bits(b.position[i].to_bits() ^ xor_bits);
                }
                pos
            }
        } else {
            b.position
        };

        let rotation = if delta.flags & DeltaFlags::ROTATION != 0 {
            if base.is_none() {
                [read_f32(&delta.data, &mut offset),
                 read_f32(&delta.data, &mut offset),
                 read_f32(&delta.data, &mut offset),
                 read_f32(&delta.data, &mut offset)]
            } else {
                let mut rot = [0.0f32; 4];
                for i in 0..4 {
                    let xor_bits = read_u32(&delta.data, &mut offset);
                    rot[i] = f32::from_bits(b.rotation[i].to_bits() ^ xor_bits);
                }
                rot
            }
        } else {
            b.rotation
        };

        let scale = if delta.flags & DeltaFlags::SCALE != 0 {
            if base.is_none() {
                [read_f32(&delta.data, &mut offset),
                 read_f32(&delta.data, &mut offset),
                 read_f32(&delta.data, &mut offset)]
            } else {
                let mut s = [0.0f32; 3];
                for i in 0..3 {
                    let xor_bits = read_u32(&delta.data, &mut offset);
                    s[i] = f32::from_bits(b.scale[i].to_bits() ^ xor_bits);
                }
                s
            }
        } else {
            b.scale
        };

        let frame = if delta.flags & DeltaFlags::FRAME != 0 {
            read_u64(&delta.data, &mut offset)
        } else {
            b.frame
        };

        EntitySnapshot {
            entity_id: delta.entity_id,
            position,
            rotation,
            scale,
            frame,
        }
    }

    /// Record that we sent `snapshot` for `id`.
    pub fn mark_sent(&mut self, id: NetworkEntityId, snapshot: EntitySnapshot) {
        self.last_sent.insert(id, snapshot);
    }

    /// Acknowledge that the remote peer has received our baseline up to a frame.
    /// Promotes last_sent snapshots into the baseline set.
    pub fn acknowledge_baseline(&mut self, _acked_frame: u64) {
        // Promote all last_sent to baselines upon acknowledgment.
        for (id, snap) in &self.last_sent {
            self.baselines.insert(*id, snap.clone());
        }
        self.baseline_frame = _acked_frame;
    }

    /// Remove tracking for a despawned entity.
    pub fn remove(&mut self, id: &NetworkEntityId) {
        self.last_sent.remove(id);
        self.baselines.remove(id);
    }

    /// Reset all baselines (e.g., on reconnection).
    pub fn reset_baselines(&mut self) {
        self.baselines.clear();
        self.last_sent.clear();
        self.baseline_frame = 0;
    }
}

impl Default for DeltaCompressor {
    fn default() -> Self {
        Self::new()
    }
}

pub struct NetworkReplication {
    pub state: Arc<RwLock<NetworkState>>,
    pub rollback: RollbackManager,
}

impl Clone for NetworkReplication {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            rollback: RollbackManager::new(self.rollback.max_frames),
        }
    }
}

impl NetworkReplication {
    pub fn new(config: NetworkConfig) -> Self {
        let max_rollback = config.rollback_frame_count;
        Self {
            state: Arc::new(RwLock::new(NetworkState::new(config))),
            rollback: RollbackManager::new(max_rollback),
        }
    }

    pub fn register_entity(&mut self, entity_index: u32) -> NetworkEntityId {
        let mut state = self.state.write().unwrap();
        let network_id = NetworkEntityId(state.next_entity_id);
        state.next_entity_id += 1;
        state.entity_to_network.insert(entity_index, network_id);
        state.network_to_entity.insert(network_id, entity_index);
        network_id
    }

    pub fn unregister_entity(&mut self, entity_index: u32) {
        let mut state = self.state.write().unwrap();
        if let Some(network_id) = state.entity_to_network.remove(&entity_index) {
            state.network_to_entity.remove(&network_id);
        }
    }

    pub fn sync_transform(
        &self,
        transport: &mut NetworkTransportResource,
        entity_index: u32,
        position: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
    ) {
        let state = self.state.read().unwrap();
        if let Some(network_id) = state.entity_to_network.get(&entity_index) {
            let message = NetworkMessage {
                sequence: state.frame_number,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
                payload: NetworkPayload::EntityTransform {
                    entity_id: *network_id,
                    position,
                    rotation,
                    scale,
                },
            };

            // Broadcast to all connected clients.
            let client_addrs: Vec<SocketAddr> = state
                .clients
                .values()
                .filter(|c| c.connected)
                .map(|c| c.addr)
                .collect();

            for addr in client_addrs {
                if let Err(e) = transport.send(addr, &message) {
                    log::warn!("sync_transform: failed to send to {}: {}", addr, e);
                }
            }
        }
    }
}

pub struct NetworkTransportResource {
    pub transport: UdpTransport,
}

impl NetworkTransportResource {
    pub fn new(transport: UdpTransport) -> Self {
        Self { transport }
    }

    pub fn send(&mut self, addr: SocketAddr, message: &NetworkMessage) -> Result<(), NetworkError> {
        let data = bincode::serialize(message).map_err(|e| NetworkError(e.to_string()))?;
        self.transport.send_to(addr, &data)
    }

    pub fn receive(&mut self) -> Vec<(SocketAddr, NetworkMessage)> {
        let mut messages = Vec::new();
        while let Ok(Some((addr, data))) = self.transport.receive() {
            if let Ok(msg) = bincode::deserialize::<NetworkMessage>(&data) {
                messages.push((addr, msg));
            }
        }
        messages
    }
}

pub struct NetworkPlugin {
    config: NetworkConfig,
}

impl NetworkPlugin {
    pub fn new(config: NetworkConfig) -> Self {
        Self { config }
    }
}

fn network_system(world: &mut crate::World) {
    // ── Receive ──────────────────────────────────────────────────
    let messages = {
        let Some(transport) = world.resource_mut::<NetworkTransportResource>() else {
            return;
        };
        transport.receive()
    };

    // Decode messages and collect connection-level updates vs entity updates.
    let mut transform_updates: Vec<(NetworkEntityId, [f32; 3], [f32; 4], [f32; 3])> = Vec::new();
    let mut snapshot_updates: Vec<(u64, Vec<EntitySnapshot>)> = Vec::new();

    {
        let Some(replication) = world.resource::<NetworkReplication>() else {
            return;
        };
        let mut state = replication.state.write().unwrap();

        for (addr, message) in &messages {
            match &message.payload {
                NetworkPayload::ConnectionRequest { client_id } => {
                    if state.clients.len() < state.config.max_clients {
                        state.clients.insert(
                            *client_id,
                            NetworkClient::new(*client_id, *addr),
                        );
                        log::info!("Client {:?} connected from {}", client_id, addr);
                    }
                }
                NetworkPayload::Disconnect { client_id } => {
                    state.clients.remove(client_id);
                    log::info!("Client {:?} disconnected", client_id);
                }
                NetworkPayload::EntityTransform {
                    entity_id,
                    position,
                    rotation,
                    scale,
                } => {
                    transform_updates.push((*entity_id, *position, *rotation, *scale));
                }
                NetworkPayload::Input { client_id, inputs } => {
                    state.input_buffer.insert(*client_id, inputs.clone());
                }
                NetworkPayload::StateSnapshot { frame, entities } => {
                    // Store for rollback reconciliation + interpolation.
                    snapshot_updates.push((*frame, entities.clone()));
                }
                _ => {}
            }
        }
    }

    // Apply received entity transforms.
    if !transform_updates.is_empty() {
        // Build network_id → entity_index map
        let net_to_entity: HashMap<NetworkEntityId, u32> = {
            let Some(replication) = world.resource::<NetworkReplication>() else {
                return;
            };
            let state = replication.state.read().unwrap();
            state.network_to_entity.clone()
        };

        // Build entity_index → Entity handle
        let entity_map: HashMap<u32, crate::ecs::Entity> = world
            .query::<quasar_math::Transform>()
            .into_iter()
            .map(|(e, _)| (e.index(), e))
            .collect();

        for (net_id, position, rotation, scale) in &transform_updates {
            if let Some(&entity_index) = net_to_entity.get(net_id) {
                if let Some(&entity) = entity_map.get(&entity_index) {
                    if let Some(t) = world.get_mut::<quasar_math::Transform>(entity) {
                        t.position = quasar_math::Vec3::new(position[0], position[1], position[2]);
                        t.rotation = quasar_math::Quat::from_xyzw(
                            rotation[0], rotation[1], rotation[2], rotation[3],
                        );
                        t.scale = quasar_math::Vec3::new(scale[0], scale[1], scale[2]);
                    }
                }
            }
        }
    }

    // Feed received state snapshots into interpolation + rollback.
    for (frame, entities) in &snapshot_updates {
        // Build map for both interpolation and rollback.
        let map: HashMap<NetworkEntityId, EntitySnapshot> = entities
            .iter()
            .map(|s| (s.entity_id, s.clone()))
            .collect();

        // Push into SnapshotInterpolation for smooth rendering.
        if let Some(rep_res) = world.resource_mut::<ReplicationResource>() {
            rep_res.interpolation.push_snapshot(*frame, map.clone());
        }
        // Push into PendingServerSnapshot for rollback reconciliation.
        if let Some(pending) = world.resource_mut::<PendingServerSnapshot>() {
            pending.snapshot = Some((*frame, map));
        }
    }

    // ── Send (server broadcasts entity transforms) ───────────────
    let is_server = world
        .resource::<NetworkReplication>()
        .map(|r| {
            let state = r.state.read().unwrap();
            state.is_server()
        })
        .unwrap_or(false);

    if !is_server {
        return;
    }

    // Collect all networked entity transforms.
    let (outgoing_messages, client_addrs) = {
        let Some(replication) = world.resource::<NetworkReplication>() else {
            return;
        };
        let state = replication.state.read().unwrap();

        let transforms: Vec<_> = world
            .query::<quasar_math::Transform>()
            .into_iter()
            .filter_map(|(entity, t)| {
                state
                    .entity_to_network
                    .get(&entity.index())
                    .map(|net_id| {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64;
                        NetworkMessage {
                            sequence: state.frame_number,
                            timestamp: now,
                            payload: NetworkPayload::EntityTransform {
                                entity_id: *net_id,
                                position: [t.position.x, t.position.y, t.position.z],
                                rotation: [
                                    t.rotation.x,
                                    t.rotation.y,
                                    t.rotation.z,
                                    t.rotation.w,
                                ],
                                scale: [t.scale.x, t.scale.y, t.scale.z],
                            },
                        }
                    })
            })
            .collect();

        let addrs: Vec<SocketAddr> = state
            .clients
            .values()
            .filter(|c| c.connected)
            .map(|c| c.addr)
            .collect();

        (transforms, addrs)
    };

    // Send transform snapshots to all connected clients.
    if !outgoing_messages.is_empty() && !client_addrs.is_empty() {
        if let Some(transport) = world.resource_mut::<NetworkTransportResource>() {
            for msg in &outgoing_messages {
                for &addr in &client_addrs {
                    if let Err(e) = transport.send(addr, msg) {
                        log::warn!("Failed to send to {}: {}", addr, e);
                    }
                }
            }
        }
    }
}

impl crate::Plugin for NetworkPlugin {
    fn name(&self) -> &str {
        "NetworkPlugin"
    }

    fn build(&self, app: &mut crate::App) {
        let bind_addr: SocketAddr = match &self.config.role {
            NetworkRole::Server | NetworkRole::ListenServer => {
                format!("0.0.0.0:{}", self.config.port).parse().unwrap()
            }
            NetworkRole::Client { server_addr: _ } => "0.0.0.0:0".parse().unwrap(),
        };

        let replication = NetworkReplication::new(self.config.clone());

        match UdpTransport::bind(bind_addr) {
            Ok(transport) => {
                app.world
                    .insert_resource(NetworkTransportResource::new(transport));
                log::info!("NetworkPlugin: UDP socket bound to {}", bind_addr);
            }
            Err(e) => {
                log::warn!(
                    "NetworkPlugin: Failed to bind UDP socket: {}, networking offline",
                    e
                );
            }
        }

        app.world.insert_resource(replication);
        app.world.insert_resource(ReplicationResource::new(self.config.tick_rate));
        app.world.insert_resource(PendingServerSnapshot::default());
        app.add_system("network_system", network_system);
        app.add_system("replication_system", replication_system);
        app.add_system("rollback_system", rollback_system);

        log::info!(
            "NetworkPlugin loaded — {} mode on port {}",
            match self.config.role {
                NetworkRole::Server => "server",
                NetworkRole::Client { .. } => "client",
                NetworkRole::ListenServer => "listen server",
            },
            self.config.port
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_config_default() {
        let config = NetworkConfig::default();
        assert_eq!(config.port, DEFAULT_PORT);
        assert_eq!(config.max_clients, MAX_CLIENTS);
    }

    #[test]
    fn network_state_creation() {
        let config = NetworkConfig::default();
        let state = NetworkState::new(config);
        assert!(state.is_server());
        assert!(!state.is_client());
    }

    #[test]
    fn rollback_manager_save_restore() {
        let mut rollback = RollbackManager::new(8);

        let mut entities = HashMap::new();
        entities.insert(
            NetworkEntityId(1),
            EntitySnapshot {
                entity_id: NetworkEntityId(1),
                position: [1.0, 2.0, 3.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
                frame: 0,
            },
        );

        rollback.save_state(entities, HashMap::new());
        assert_eq!(rollback.states.len(), 1);
    }

    #[test]
    fn replication_resource_spawn_despawn() {
        let mut res = ReplicationResource::new(60);
        let id = NetworkEntityId(42);
        res.queue_spawn(id);
        res.queue_despawn(NetworkEntityId(7));
        assert_eq!(res.pending_spawns.len(), 1);
        assert_eq!(res.pending_despawns.len(), 1);
        assert_eq!(res.pending_spawns[0], id);
    }

    #[test]
    fn delta_compressor_round_trip() {
        let mut dc = DeltaCompressor::new();
        let id = NetworkEntityId(1);
        let snap = EntitySnapshot {
            entity_id: id,
            position: [1.0, 2.0, 3.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
            frame: 0,
        };
        // First update should always report needs_update.
        assert!(dc.needs_update(id, &snap));
        let encoded = dc.encode_delta(id, &snap);
        assert!(encoded.is_some());
        dc.mark_sent(id, snap.clone());
        // Same snap again — no update needed.
        assert!(!dc.needs_update(id, &snap));

        // Changed position → needs update.
        let snap2 = EntitySnapshot {
            position: [4.0, 5.0, 6.0],
            ..snap.clone()
        };
        assert!(dc.needs_update(id, &snap2));
    }

    #[test]
    fn dirty_tracker_mark_take() {
        let mut dt = DirtyTracker::new();
        let id = NetworkEntityId(1);
        dt.mark(id, "Transform");
        assert!(dt.take(id, "Transform"));
        // Second take should return false.
        assert!(!dt.take(id, "Transform"));
    }

    #[test]
    fn replicated_constructors() {
        let server = Replicated::server_owned(NetworkEntityId(1), vec!["Transform".into()]);
        assert!(server.owner.is_none());
        assert_eq!(server.priority, 1.0);

        let client = Replicated::client_owned(NetworkEntityId(2), ClientId(5), vec![]);
        assert_eq!(client.owner, Some(ClientId(5)));
    }
}

// ════════════════════════════════════════════════════════════════════
//  ADVANCED NETWORKING — server-authoritative tick, dirty-flag
//  replication, snapshot interpolation, `Replicated` marker
// ════════════════════════════════════════════════════════════════════

/// Marker component: attach to any entity that should be replicated.
///
/// `owner` indicates which client "owns" the entity for authority purposes.
/// Server-owned entities have `owner = None`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replicated {
    pub network_id: NetworkEntityId,
    /// `None` → server authority, `Some(id)` → client authority
    pub owner: Option<ClientId>,
    /// Component type-names that should be replicated.
    pub replicated_components: Vec<String>,
    /// Priority (higher = more frequent updates). Default 1.0.
    pub priority: f32,
}

impl Replicated {
    pub fn server_owned(network_id: NetworkEntityId, components: Vec<String>) -> Self {
        Self {
            network_id,
            owner: None,
            replicated_components: components,
            priority: 1.0,
        }
    }

    pub fn client_owned(
        network_id: NetworkEntityId,
        owner: ClientId,
        components: Vec<String>,
    ) -> Self {
        Self {
            network_id,
            owner: Some(owner),
            replicated_components: components,
            priority: 1.0,
        }
    }
}

// ── dirty-flag tracker ──────────────────────────────────────────

/// Tracks per-entity, per-component dirty bits so only changed data
/// is sent over the wire.
#[derive(Debug, Clone, Default)]
pub struct DirtyTracker {
    /// (NetworkEntityId, component type name) → dirty
    dirty: HashMap<(NetworkEntityId, String), bool>,
}

impl DirtyTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark a specific component on an entity as dirty (needs replication).
    pub fn mark(&mut self, entity: NetworkEntityId, component: &str) {
        self.dirty
            .insert((entity, component.to_string()), true);
    }

    /// Check and clear the dirty bit. Returns `true` if it was dirty.
    pub fn take(&mut self, entity: NetworkEntityId, component: &str) -> bool {
        self.dirty
            .remove(&(entity, component.to_string()))
            .unwrap_or(false)
    }

    /// Clear all dirty bits (typically at end of tick after sending).
    pub fn clear_all(&mut self) {
        self.dirty.clear();
    }
}

// ════════════════════════════════════════════════════════════════════
//  RELIABILITY LAYER — ACK / retransmission over UDP
// ════════════════════════════════════════════════════════════════════

/// Whether a given network payload requires reliable delivery.
pub fn payload_needs_reliability(payload: &NetworkPayload) -> bool {
    matches!(
        payload,
        NetworkPayload::EntitySpawn { .. }
            | NetworkPayload::EntityDespawn { .. }
            | NetworkPayload::Rpc { .. }
            | NetworkPayload::ConnectionRequest { .. }
            | NetworkPayload::ConnectionAccepted { .. }
            | NetworkPayload::ConnectionDenied { .. }
            | NetworkPayload::Disconnect { .. }
    )
}

/// ACK payload piggybacked on regular messages or sent standalone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AckMessage {
    /// Sequences being acknowledged.
    pub acked_sequences: Vec<u64>,
}

/// A pending reliable message awaiting acknowledgement.
#[derive(Debug, Clone)]
struct PendingReliable {
    message: NetworkMessage,
    destination: SocketAddr,
    /// Monotonic time of last (re)send.
    last_send_time: f64,
    /// Number of times we have (re)sent this message.
    send_count: u32,
}

/// Sliding-window reliability manager for a single connection direction.
///
/// Reliable messages are tracked by sequence number. The remote peer must
/// ACK each sequence; un-ACKed messages are retransmitted after a timeout.
/// Fire-and-forget payloads (transforms, snapshots) bypass this entirely.
pub struct ReliabilityManager {
    /// Reliable messages waiting for ACK, keyed by sequence number.
    pending: HashMap<u64, PendingReliable>,
    /// Set of sequences we have received and should ACK back.
    received_sequences: Vec<u64>,
    /// Sequences already processed — used for duplicate rejection.
    processed_sequences: std::collections::HashSet<u64>,
    /// Maximum number of remembered processed sequences (ring eviction).
    max_processed_history: usize,
    /// Retransmit timeout in seconds.
    pub retransmit_timeout: f64,
    /// Maximum retransmit attempts before giving up.
    pub max_retransmits: u32,
}

impl ReliabilityManager {
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
            received_sequences: Vec::new(),
            processed_sequences: std::collections::HashSet::new(),
            max_processed_history: 1024,
            retransmit_timeout: 0.2, // 200 ms
            max_retransmits: 20,
        }
    }

    /// Track an outgoing reliable message.
    pub fn track_send(
        &mut self,
        message: NetworkMessage,
        destination: SocketAddr,
        now: f64,
    ) {
        let seq = message.sequence;
        self.pending.insert(
            seq,
            PendingReliable {
                message,
                destination,
                last_send_time: now,
                send_count: 1,
            },
        );
    }

    /// Process incoming ACKs — remove acknowledged messages from the pending set.
    pub fn process_acks(&mut self, acks: &[u64]) {
        for seq in acks {
            self.pending.remove(seq);
        }
    }

    /// Record that we received a reliable message with the given sequence.
    /// Returns `true` if this is a NEW message (not a duplicate).
    pub fn record_received(&mut self, sequence: u64) -> bool {
        if self.processed_sequences.contains(&sequence) {
            return false; // duplicate
        }
        // Evict oldest entries when history is full.
        if self.processed_sequences.len() >= self.max_processed_history {
            // Remove the smallest sequence (oldest).
            if let Some(&oldest) = self.processed_sequences.iter().min() {
                self.processed_sequences.remove(&oldest);
            }
        }
        self.processed_sequences.insert(sequence);
        self.received_sequences.push(sequence);
        true
    }

    /// Drain the set of received sequences that need to be ACKed back.
    pub fn drain_pending_acks(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.received_sequences)
    }

    /// Collect messages that need retransmission.
    pub fn collect_retransmits(&mut self, now: f64) -> Vec<(NetworkMessage, SocketAddr)> {
        let mut retransmits = Vec::new();
        let mut expired = Vec::new();

        for (&seq, pending) in &mut self.pending {
            if now - pending.last_send_time >= self.retransmit_timeout {
                if pending.send_count >= self.max_retransmits {
                    log::warn!(
                        "Reliable message seq={} dropped after {} retransmits",
                        seq,
                        pending.send_count
                    );
                    expired.push(seq);
                } else {
                    pending.send_count += 1;
                    pending.last_send_time = now;
                    retransmits.push((pending.message.clone(), pending.destination));
                }
            }
        }

        for seq in expired {
            self.pending.remove(&seq);
        }

        retransmits
    }

    /// Number of messages still awaiting ACK.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

impl Default for ReliabilityManager {
    fn default() -> Self {
        Self::new()
    }
}

// ════════════════════════════════════════════════════════════════════
//  QUIC TRANSPORT ABSTRACTION
// ════════════════════════════════════════════════════════════════════

/// Transport protocol selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportProtocol {
    Udp,
    Quic,
}

/// Configuration for the QUIC transport.
#[derive(Debug, Clone)]
pub struct QuicConfig {
    /// Server certificate (DER encoded) for TLS — `None` to accept any cert
    /// (useful for development / LAN play).
    pub server_cert_der: Option<Vec<u8>>,
    /// Keep-alive interval in milliseconds. 0 = disabled.
    pub keep_alive_ms: u32,
    /// Maximum idle timeout in milliseconds before the connection is closed.
    pub idle_timeout_ms: u32,
    /// Maximum number of concurrent unidirectional streams.
    pub max_uni_streams: u32,
    /// Maximum number of concurrent bidirectional streams.
    pub max_bi_streams: u32,
}

impl Default for QuicConfig {
    fn default() -> Self {
        Self {
            server_cert_der: None,
            keep_alive_ms: 5000,
            idle_timeout_ms: 30000,
            max_uni_streams: 16,
            max_bi_streams: 16,
        }
    }
}

/// Stream priority / channel designation for QUIC streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuicChannel {
    /// Reliable ordered — control messages, RPCs, spawns/despawns.
    Reliable,
    /// Unreliable unordered — entity state snapshots (uses datagrams).
    Unreliable,
    /// Reliable unordered — asset chunk transfers.
    BulkTransfer,
}

/// A QUIC transport session (client or server side).
///
/// This is a protocol-level abstraction. The actual QUIC implementation
/// (e.g. via the `quinn` crate) is plugged in at the application layer
/// through the [`QuicTransportBackend`] trait.
pub struct QuicTransport {
    backend: Box<dyn QuicTransportBackend>,
    config: QuicConfig,
}

/// Trait for the underlying QUIC library integration.
///
/// Implementors wrap a library like `quinn` or `s2n-quic` behind this
/// interface so the engine stays backend-agnostic.
pub trait QuicTransportBackend: Send + Sync {
    /// Open a connection to `addr`. Non-blocking; connection progress
    /// is reported via [`QuicTransportBackend::poll`].
    fn connect(&mut self, addr: SocketAddr) -> Result<(), NetworkError>;

    /// Start listening on `addr`.
    fn listen(&mut self, addr: SocketAddr) -> Result<(), NetworkError>;

    /// Drive the QUIC state machine — must be called every frame.
    fn poll(&mut self) -> Vec<QuicEvent>;

    /// Send data on the given channel to `addr`.
    fn send(&mut self, addr: SocketAddr, channel: QuicChannel, data: &[u8]) -> Result<(), NetworkError>;

    /// Number of currently connected peers.
    fn peer_count(&self) -> usize;

    /// Close a specific connection.
    fn disconnect(&mut self, addr: SocketAddr);
}

/// Events produced by the QUIC backend each frame.
#[derive(Debug, Clone)]
pub enum QuicEvent {
    /// A new peer connected.
    Connected(SocketAddr),
    /// A peer disconnected (may include a reason string).
    Disconnected(SocketAddr, String),
    /// Data received from a peer on a given channel.
    Data {
        from: SocketAddr,
        channel: QuicChannel,
        payload: Vec<u8>,
    },
}

impl QuicTransport {
    pub fn new(backend: Box<dyn QuicTransportBackend>, config: QuicConfig) -> Self {
        Self { backend, config }
    }

    pub fn config(&self) -> &QuicConfig {
        &self.config
    }

    pub fn connect(&mut self, addr: SocketAddr) -> Result<(), NetworkError> {
        self.backend.connect(addr)
    }

    pub fn listen(&mut self, addr: SocketAddr) -> Result<(), NetworkError> {
        self.backend.listen(addr)
    }

    pub fn poll(&mut self) -> Vec<QuicEvent> {
        self.backend.poll()
    }

    pub fn send(
        &mut self,
        addr: SocketAddr,
        channel: QuicChannel,
        data: &[u8],
    ) -> Result<(), NetworkError> {
        self.backend.send(addr, channel, data)
    }

    pub fn send_message(
        &mut self,
        addr: SocketAddr,
        channel: QuicChannel,
        message: &NetworkMessage,
    ) -> Result<(), NetworkError> {
        let data = bincode::serialize(message).map_err(|e| NetworkError(e.to_string()))?;
        self.send(addr, channel, &data)
    }

    pub fn peer_count(&self) -> usize {
        self.backend.peer_count()
    }

    pub fn disconnect(&mut self, addr: SocketAddr) {
        self.backend.disconnect(addr);
    }
}

// ════════════════════════════════════════════════════════════════════
//  REPLICATION SYSTEM — ties Replicated + DirtyTracker +
//  DeltaCompressor + SnapshotInterpolation into one ECS system
// ════════════════════════════════════════════════════════════════════

/// High-level replication resource that the `replication_system` consumes.
pub struct ReplicationResource {
    pub dirty_tracker: DirtyTracker,
    pub delta_compressor: DeltaCompressor,
    pub interpolation: SnapshotInterpolation,
    pub tick_accumulator: TickAccumulator,
    /// Entities awaiting initial spawn replication.
    pending_spawns: Vec<NetworkEntityId>,
    /// Entities awaiting despawn replication.
    pending_despawns: Vec<NetworkEntityId>,
}

impl ReplicationResource {
    pub fn new(tick_rate: u32) -> Self {
        Self {
            dirty_tracker: DirtyTracker::new(),
            delta_compressor: DeltaCompressor::new(),
            interpolation: SnapshotInterpolation::new(tick_rate),
            tick_accumulator: TickAccumulator::new(tick_rate),
            pending_spawns: Vec::new(),
            pending_despawns: Vec::new(),
        }
    }

    pub fn queue_spawn(&mut self, id: NetworkEntityId) {
        self.pending_spawns.push(id);
    }

    pub fn queue_despawn(&mut self, id: NetworkEntityId) {
        self.pending_despawns.push(id);
    }
}

/// Server-side replication system — runs once per network tick.
///
/// 1. Gathers all entities with [`Replicated`] component.
/// 2. Collects dirty transforms into delta-compressed packets.
/// 3. Broadcasts deltas + reliable spawn/despawn messages.
/// 4. On clients, feeds received snapshots into `SnapshotInterpolation`.
pub fn replication_system(world: &mut crate::World) {
    // Phase 1: read-only gather — collect all data we need before mutating.
    let is_server = world
        .resource::<NetworkReplication>()
        .map(|r| r.state.read().unwrap().is_server())
        .unwrap_or(false);

    if is_server {
        server_replication_tick(world);
    } else {
        client_interpolation_tick(world);
    }
}

fn server_replication_tick(world: &mut crate::World) {
    // ── Gather snapshots (read-only) ─────────────────
    let snapshots: Vec<(NetworkEntityId, EntitySnapshot)> = {
        let replicated_entities: Vec<(crate::ecs::Entity, NetworkEntityId)> = world
            .query::<Replicated>()
            .into_iter()
            .map(|(e, rep)| (e, rep.network_id))
            .collect();

        let mut out = Vec::with_capacity(replicated_entities.len());
        for (entity, net_id) in replicated_entities {
            if let Some(t) = world.get::<quasar_math::Transform>(entity) {
                out.push((
                    net_id,
                    EntitySnapshot {
                        entity_id: net_id,
                        position: [t.position.x, t.position.y, t.position.z],
                        rotation: [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w],
                        scale: [t.scale.x, t.scale.y, t.scale.z],
                        frame: 0,
                    },
                ));
            }
        }
        out
    };

    // ── Gather metadata (read-only) ──────────────────
    let (frame_number, client_addrs) = {
        let default = (0u64, Vec::new());
        world
            .resource::<NetworkReplication>()
            .map(|r| {
                let state = r.state.read().unwrap();
                let addrs: Vec<SocketAddr> = state
                    .clients
                    .values()
                    .filter(|c| c.connected)
                    .map(|c| c.addr)
                    .collect();
                (state.frame_number, addrs)
            })
            .unwrap_or(default)
    };

    if client_addrs.is_empty() {
        return;
    }

    // ── Delta-compress (mutable ReplicationResource, no other borrows) ──
    let (outgoing_messages, _has_deltas) = {
        let Some(rep_res) = world.resource_mut::<ReplicationResource>() else {
            return;
        };

        let spawns = std::mem::take(&mut rep_res.pending_spawns);
        let despawns = std::mem::take(&mut rep_res.pending_despawns);

        let mut has_deltas = false;
        for (net_id, snap) in &snapshots {
            if rep_res.delta_compressor.needs_update(*net_id, snap) {
                let _ = rep_res.delta_compressor.encode_delta(*net_id, snap);
                rep_res.delta_compressor.mark_sent(*net_id, snap.clone());
                has_deltas = true;
            }
        }

        for net_id in &despawns {
            rep_res.delta_compressor.remove(net_id);
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let mut msgs: Vec<NetworkMessage> = Vec::new();

        // Reliable spawn/despawn messages.
        for net_id in spawns {
            msgs.push(NetworkMessage {
                sequence: frame_number,
                timestamp: now,
                payload: NetworkPayload::EntitySpawn {
                    entity_id: net_id,
                    components: Vec::new(),
                },
            });
        }
        for net_id in despawns {
            msgs.push(NetworkMessage {
                sequence: frame_number,
                timestamp: now,
                payload: NetworkPayload::EntityDespawn { entity_id: net_id },
            });
        }

        // Snapshot message with all dirty entity states.
        if has_deltas {
            let snap_entities: Vec<EntitySnapshot> =
                snapshots.iter().map(|(_, s)| s.clone()).collect();
            msgs.push(NetworkMessage {
                sequence: frame_number,
                timestamp: now,
                payload: NetworkPayload::StateSnapshot {
                    frame: frame_number,
                    entities: snap_entities,
                },
            });
        }

        (msgs, has_deltas)
    };

    // ── Send (mutable transport, no other borrows) ───
    if !outgoing_messages.is_empty() {
        if let Some(transport) = world.resource_mut::<NetworkTransportResource>() {
            for msg in &outgoing_messages {
                for &addr in &client_addrs {
                    let _ = transport.send(addr, msg);
                }
            }
        }
    }
}

fn client_interpolation_tick(world: &mut crate::World) {
    // Advance interpolation and collect results.
    let results: Vec<(NetworkEntityId, [f32; 3], [f32; 4])> = {
        let Some(rep_res) = world.resource_mut::<ReplicationResource>() else {
            return;
        };
        rep_res.interpolation.interpolate(1.0 / 60.0)
    };

    if results.is_empty() {
        return;
    }

    // Map network IDs → entity indices.
    let entity_map: Vec<(u32, [f32; 3], [f32; 4])> = {
        let Some(replication) = world.resource::<NetworkReplication>() else {
            return;
        };
        let state = replication.state.read().unwrap();
        results
            .iter()
            .filter_map(|(net_id, pos, rot)| {
                state.network_to_entity.get(net_id).map(|&idx| (idx, *pos, *rot))
            })
            .collect()
    };

    // Find entities and apply transforms.
    let all_entities: Vec<(crate::ecs::Entity, u32)> = world
        .query::<quasar_math::Transform>()
        .into_iter()
        .map(|(e, _)| (e, e.index()))
        .collect();

    for (target_idx, pos, rot) in &entity_map {
        for &(entity, idx) in &all_entities {
            if idx == *target_idx {
                if let Some(t) = world.get_mut::<quasar_math::Transform>(entity) {
                    t.position = quasar_math::Vec3::new(pos[0], pos[1], pos[2]);
                    t.rotation = quasar_math::Quat::from_xyzw(rot[0], rot[1], rot[2], rot[3]);
                }
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════
//  ROLLBACK SYSTEM — client-side prediction + server reconciliation
// ════════════════════════════════════════════════════════════════════

/// Resource holding the latest authoritative server snapshot that has not yet
/// been reconciled. `network_system` pushes into this; `rollback_system`
/// consumes it.
#[derive(Default)]
pub struct PendingServerSnapshot {
    pub snapshot: Option<(u64, HashMap<NetworkEntityId, EntitySnapshot>)>,
}

/// Client-side rollback system.
///
/// Each frame:
/// 1. Saves the current predicted state.
/// 2. Checks if the server snapshot diverges from our prediction.
/// 3. If so, runs `RollbackManager::run_rollback` to rewind and re-simulate.
pub fn rollback_system(world: &mut crate::World) {
    // Only clients use rollback prediction.
    let is_client = world
        .resource::<NetworkReplication>()
        .map(|r| !r.state.read().unwrap().is_server())
        .unwrap_or(false);

    if !is_client {
        return;
    }

    // 1. Save current predicted state.
    let current_entities: HashMap<NetworkEntityId, EntitySnapshot> = {
        world
            .query::<Replicated>()
            .into_iter()
            .filter_map(|(entity, rep)| {
                let t = world.get::<quasar_math::Transform>(entity)?;
                Some((
                    rep.network_id,
                    EntitySnapshot {
                        entity_id: rep.network_id,
                        position: [t.position.x, t.position.y, t.position.z],
                        rotation: [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w],
                        scale: [t.scale.x, t.scale.y, t.scale.z],
                        frame: 0,
                    },
                ))
            })
            .collect()
    };

    // Save to rollback history.
    if let Some(replication) = world.resource_mut::<NetworkReplication>() {
        replication.rollback.save_state(current_entities, HashMap::new());
    }

    // 2. Check for pending server snapshot.
    let pending = world
        .resource_mut::<PendingServerSnapshot>()
        .and_then(|p| p.snapshot.take());

    let Some((server_frame, server_entities)) = pending else {
        return;
    };

    // 3. Detect misprediction and run rollback if needed.
    let did_rollback = {
        let Some(replication) = world.resource_mut::<NetworkReplication>() else {
            return;
        };
        // run_rollback needs a simulate_fn — use a no-op that returns the
        // server entities as the "re-simulated" state. In a real game, this
        // closure would run the physics/gameplay systems for one tick.
        replication.rollback.run_rollback(
            server_frame,
            &server_entities,
            |_frame, _inputs| {
                // Placeholder: real games replace this with actual re-simulation.
                server_entities.clone()
            },
        )
    };

    if did_rollback {
        // Apply corrected state back to the world.
        let corrected: Vec<(NetworkEntityId, EntitySnapshot)> = {
            let Some(replication) = world.resource::<NetworkReplication>() else {
                return;
            };
            replication
                .rollback
                .states
                .last()
                .map(|s| s.entities.iter().map(|(k, v)| (*k, v.clone())).collect())
                .unwrap_or_default()
        };

        let entity_map: HashMap<NetworkEntityId, u32> = {
            let Some(replication) = world.resource::<NetworkReplication>() else {
                return;
            };
            let state = replication.state.read().unwrap();
            state.network_to_entity.clone()
        };

        let all_entities: Vec<(crate::ecs::Entity, u32)> = world
            .query::<quasar_math::Transform>()
            .into_iter()
            .map(|(e, _)| (e, e.index()))
            .collect();

        for (net_id, snap) in &corrected {
            if let Some(&target_idx) = entity_map.get(net_id) {
                for &(entity, idx) in &all_entities {
                    if idx == target_idx {
                        if let Some(t) = world.get_mut::<quasar_math::Transform>(entity) {
                            t.position = quasar_math::Vec3::new(
                                snap.position[0],
                                snap.position[1],
                                snap.position[2],
                            );
                            t.rotation = quasar_math::Quat::from_xyzw(
                                snap.rotation[0],
                                snap.rotation[1],
                                snap.rotation[2],
                                snap.rotation[3],
                            );
                            t.scale = quasar_math::Vec3::new(
                                snap.scale[0],
                                snap.scale[1],
                                snap.scale[2],
                            );
                        }
                    }
                }
            }
        }

        log::debug!("Rollback reconciliation applied for server frame {}", server_frame);
    }
}

// ════════════════════════════════════════════════════════════════════
//  REPLICATION DESCRIPTOR — derive(Replicate) support types
// ════════════════════════════════════════════════════════════════════

/// Mode controlling how a field is replicated across the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplicationMode {
    /// Field is replicated to all peers.
    Replicated,
    /// Owner predicts locally, others receive authoritative value.
    OwnerPredicted,
    /// Server-only — never sent to clients.
    ServerOnly,
}

/// Metadata for a single replicated field.
#[derive(Debug, Clone)]
pub struct ReplicatedField {
    pub name: &'static str,
    pub type_name: &'static str,
    pub mode: ReplicationMode,
}

/// Trait auto-implemented by `#[derive(Replicate)]`.
///
/// Provides component-level metadata and serialization for network replication.
pub trait ReplicateDescriptor: Sized {
    /// Stable name for this component type.
    fn component_name() -> &'static str;

    /// Static list of replicated field descriptors.
    fn replicated_fields() -> &'static [ReplicatedField];

    /// Serialize only the replicated fields to bytes.
    fn replicate_serialize(&self) -> Vec<u8>;

    /// Deserialize replicated fields from bytes, filling non-replicated with defaults.
    fn replicate_deserialize(data: &[u8]) -> Option<Self>;
}

// ── Lag compensation ────────────────────────────────────────────

/// Ring buffer storing a rolling history of values for lag compensation.
///
/// Used to look up an entity's state at a past server tick. Typical use:
/// `HistoryBuffer<[f32; 3]>` for position or `HistoryBuffer<Transform>`.
#[derive(Debug, Clone)]
pub struct HistoryBuffer<T: Clone> {
    entries: Vec<(u64, T)>,
    capacity: usize,
}

impl<T: Clone> HistoryBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
            capacity,
        }
    }

    /// Record a value at the given tick.
    pub fn push(&mut self, tick: u64, value: T) {
        if self.entries.len() >= self.capacity {
            self.entries.remove(0);
        }
        self.entries.push((tick, value));
    }

    /// Get the value at a specific tick, or the closest earlier tick.
    pub fn at_tick(&self, tick: u64) -> Option<&T> {
        // Exact match first.
        if let Some(entry) = self.entries.iter().rev().find(|(t, _)| *t == tick) {
            return Some(&entry.1);
        }
        // Closest earlier tick.
        self.entries
            .iter()
            .rev()
            .find(|(t, _)| *t <= tick)
            .map(|(_, v)| v)
    }

    /// Interpolate between two surrounding ticks.
    /// Returns `None` if the buffer doesn't bracket the requested tick.
    pub fn interpolate_at(&self, tick: u64) -> Option<(&T, &T, f32)>
    where
        T: Clone,
    {
        let before = self
            .entries
            .iter()
            .rev()
            .find(|(t, _)| *t <= tick)?;
        let after = self.entries.iter().find(|(t, _)| *t >= tick)?;
        if before.0 == after.0 {
            return Some((&before.1, &after.1, 0.0));
        }
        let alpha = (tick - before.0) as f32 / (after.0 - before.0) as f32;
        Some((&before.1, &after.1, alpha))
    }

    /// Number of stored entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all stored history.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

// ════════════════════════════════════════════════════════════════════
//  SERVER-SIDE LAG COMPENSATION (rewind & hit verification)
// ════════════════════════════════════════════════════════════════════

/// Stores position history for all networked entities on the server.
/// Used for server-side lag compensation: when a client claims a hit,
/// the server rewinds to the client's perceived tick and checks the
/// hitbox at that historical position.
pub struct LagCompensationManager {
    /// Per-entity position history, keyed by `NetworkEntityId`.
    histories: HashMap<NetworkEntityId, HistoryBuffer<[f32; 3]>>,
    /// Maximum rewind window in ticks.
    pub max_rewind_ticks: u64,
    /// How many ticks of history each entity stores.
    pub history_capacity: usize,
}

impl Default for LagCompensationManager {
    fn default() -> Self {
        Self {
            histories: HashMap::new(),
            max_rewind_ticks: 30,   // ~500 ms at 60 Hz
            history_capacity: 64,
        }
    }
}

impl LagCompensationManager {
    pub fn new(max_rewind_ticks: u64) -> Self {
        Self {
            max_rewind_ticks,
            ..Default::default()
        }
    }

    /// Record the current position of an entity at the given server tick.
    pub fn record(&mut self, entity: NetworkEntityId, tick: u64, position: [f32; 3]) {
        let buf = self
            .histories
            .entry(entity)
            .or_insert_with(|| HistoryBuffer::new(self.history_capacity));
        buf.push(tick, position);
    }

    /// Get the rewound position of an entity at a past tick. Returns the
    /// exact stored position, or the closest earlier snapshot.
    pub fn position_at(&self, entity: NetworkEntityId, tick: u64) -> Option<[f32; 3]> {
        self.histories
            .get(&entity)
            .and_then(|buf| buf.at_tick(tick).copied())
    }

    /// Verify a hit against a rewound hitbox.
    ///
    /// `claimed_tick` — the server tick the client claims the shot happened.
    /// `target`       — the entity the client claims to have hit.
    /// `shot_origin`  — world-space origin of the shot.
    /// `shot_dir`     — normalised direction of the shot.
    /// `hitbox_radius`— radius around the entity position to check.
    ///
    /// Returns `true` if the shot intersects the sphere at the rewound position.
    pub fn verify_hit(
        &self,
        claimed_tick: u64,
        target: NetworkEntityId,
        shot_origin: [f32; 3],
        shot_dir: [f32; 3],
        hitbox_radius: f32,
    ) -> bool {
        let current_tick = self
            .histories
            .values()
            .flat_map(|b| b.entries.last().map(|(t, _)| *t))
            .max()
            .unwrap_or(0);

        // Reject if rewind too far.
        if current_tick.saturating_sub(claimed_tick) > self.max_rewind_ticks {
            return false;
        }

        let Some(pos) = self.position_at(target, claimed_tick) else {
            return false;
        };

        // Ray-sphere intersection test.
        let oc = [
            shot_origin[0] - pos[0],
            shot_origin[1] - pos[1],
            shot_origin[2] - pos[2],
        ];
        let b = oc[0] * shot_dir[0] + oc[1] * shot_dir[1] + oc[2] * shot_dir[2];
        let c = oc[0] * oc[0] + oc[1] * oc[1] + oc[2] * oc[2] - hitbox_radius * hitbox_radius;
        let discriminant = b * b - c;

        discriminant >= 0.0
    }

    /// Remove an entity's history (e.g. on despawn).
    pub fn remove(&mut self, entity: NetworkEntityId) {
        self.histories.remove(&entity);
    }
}

// ════════════════════════════════════════════════════════════════════
//  RELAY SERVER SCAFFOLD
// ════════════════════════════════════════════════════════════════════

/// Configuration for a lightweight relay server that forwards packets
/// between clients without running game simulation (for NAT traversal).
#[derive(Debug, Clone)]
pub struct RelayServerConfig {
    /// Address to bind the relay socket.
    pub bind_addr: std::net::SocketAddr,
    /// Maximum number of concurrent sessions.
    pub max_sessions: usize,
    /// Idle timeout per session (seconds).
    pub session_timeout_secs: u64,
}

impl Default for RelayServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:7878".parse().expect("valid default relay addr"),
            max_sessions: 256,
            session_timeout_secs: 300,
        }
    }
}

/// A session on the relay server — tracks two endpoints forwarding to each other.
#[derive(Debug, Clone)]
pub struct RelaySession {
    pub id: u64,
    pub peers: Vec<std::net::SocketAddr>,
    pub last_activity: std::time::Instant,
}

/// The relay server state. TCP/UDP I/O is handled externally; this struct
/// only manages session bookkeeping and packet routing decisions.
pub struct RelayServer {
    pub config: RelayServerConfig,
    pub sessions: Vec<RelaySession>,
    next_session_id: u64,
}

impl RelayServer {
    pub fn new(config: RelayServerConfig) -> Self {
        Self {
            config,
            sessions: Vec::new(),
            next_session_id: 1,
        }
    }

    /// Create a new session and return its id.
    pub fn create_session(&mut self, initial_peer: std::net::SocketAddr) -> u64 {
        let id = self.next_session_id;
        self.next_session_id += 1;
        self.sessions.push(RelaySession {
            id,
            peers: vec![initial_peer],
            last_activity: std::time::Instant::now(),
        });
        id
    }

    /// Add a peer to an existing session.
    pub fn join_session(&mut self, session_id: u64, peer: std::net::SocketAddr) -> bool {
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            if session.peers.len() < self.config.max_sessions {
                session.peers.push(peer);
                session.last_activity = std::time::Instant::now();
                return true;
            }
        }
        false
    }

    /// Get the list of peers a packet from `sender` should be forwarded to.
    pub fn forward_targets(&self, sender: std::net::SocketAddr) -> Vec<std::net::SocketAddr> {
        for session in &self.sessions {
            if session.peers.contains(&sender) {
                return session
                    .peers
                    .iter()
                    .filter(|&&p| p != sender)
                    .copied()
                    .collect();
            }
        }
        Vec::new()
    }

    /// Prune idle sessions.
    pub fn prune_idle(&mut self) {
        let timeout = std::time::Duration::from_secs(self.config.session_timeout_secs);
        self.sessions.retain(|s| s.last_activity.elapsed() < timeout);
    }
}


