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
