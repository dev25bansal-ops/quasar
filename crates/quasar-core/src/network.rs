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

/// **STUB** — Rollback netcode manager.
///
/// `RollbackManager` provides state snapshot storage and retrieval, but the
/// following features are **not yet implemented**:
///
/// - **Input history buffer**: upstream must feed per-frame inputs via
///   [`save_state`](Self::save_state).
/// - **Deterministic simulation tick**: the physics / game-logic step must
///   be fully deterministic for rollback to produce correct results.
/// - **Re-simulation after misprediction**: when a server-authoritative
///   snapshot disagrees with a predicted frame, the caller is responsible
///   for calling [`rollback_to`](Self::rollback_to) and re-simulating
///   forward with corrected inputs.
///
/// `NetworkConfig::rollback_frame_count` controls how many frames of
/// history are kept (default 8).  This is sufficient for typical
/// round-trip latencies ≤130 ms at 60 Hz.
///
/// **Next steps** (in order of priority):
/// 1. Add an `InputHistory` ring-buffer that stores per-client inputs.
/// 2. Teach the server tick loop to call `save_state` every tick.
/// 3. On receiving a correction, rewind via `rollback_to` and re-simulate
///    with the corrected inputs, then fast-forward back to the current
///    frame.
pub struct RollbackManager {
    pub states: Vec<RollbackState>,
    pub max_frames: u32,
    pub current_frame: u64,
}

impl RollbackManager {
    pub fn new(max_frames: u32) -> Self {
        Self {
            states: Vec::with_capacity(max_frames as usize),
            max_frames,
            current_frame: 0,
        }
    }

    pub fn save_state(
        &mut self,
        entities: HashMap<NetworkEntityId, EntitySnapshot>,
        inputs: HashMap<ClientId, Vec<InputData>>,
    ) {
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
pub struct DeltaCompressor {
    last_sent: HashMap<NetworkEntityId, EntitySnapshot>,
}

impl DeltaCompressor {
    pub fn new() -> Self {
        Self {
            last_sent: HashMap::new(),
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

    /// Record that we sent `snapshot` for `id`.
    pub fn mark_sent(&mut self, id: NetworkEntityId, snapshot: EntitySnapshot) {
        self.last_sent.insert(id, snapshot);
    }

    /// Remove tracking for a despawned entity.
    pub fn remove(&mut self, id: &NetworkEntityId) {
        self.last_sent.remove(id);
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
        app.add_system("network_system", network_system);

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


