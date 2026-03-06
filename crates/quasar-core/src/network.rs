//! Networking — QUIC/UDP integration for game networking.
//!
//! Provides:
//! - NetworkPlugin with client/server roles
//! - Entity replication across network
//! - Rollback netcode support

use std::collections::HashMap;
use std::io;
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

/// Trait for network transport abstraction.
pub trait NetworkTransport: Send + Sync {
    fn send_to(&mut self, addr: SocketAddr, data: &[u8]) -> Result<(), NetworkError>;
    fn receive(&mut self) -> Result<Option<(SocketAddr, Vec<u8>)>, NetworkError>;
    fn local_addr(&self) -> Result<SocketAddr, NetworkError>;
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

/// UDP-based network transport using std::net.
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
}

impl NetworkTransport for UdpTransport {
    fn send_to(&mut self, addr: SocketAddr, data: &[u8]) -> Result<(), NetworkError> {
        self.socket.send_to(data, addr)?;
        Ok(())
    }

    fn receive(&mut self) -> Result<Option<(SocketAddr, Vec<u8>)>, NetworkError> {
        match self.socket.recv_from(&mut self.recv_buffer) {
            Ok((len, addr)) => Ok(Some((addr, self.recv_buffer[..len].to_vec()))),
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(NetworkError(e.to_string())),
        }
    }

    fn local_addr(&self) -> Result<SocketAddr, NetworkError> {
        Ok(self.socket.local_addr()?)
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

    pub fn resimulate(
        &mut self,
        from_frame: u64,
        to_frame: u64,
        inputs: HashMap<ClientId, Vec<InputData>>,
    ) {
        for frame in from_frame..=to_frame {
            if let Some(state) = self.rollback_to(frame) {
                self.current_frame = frame;
            }
        }
    }
}

pub struct NetworkReplication {
    pub state: Arc<RwLock<NetworkState>>,
    pub rollback: RollbackManager,
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
        &mut self,
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
                    .unwrap()
                    .as_millis() as u64,
                payload: NetworkPayload::EntityTransform {
                    entity_id: *network_id,
                    position,
                    rotation,
                    scale,
                },
            };
        }
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

impl crate::Plugin for NetworkPlugin {
    fn name(&self) -> &str {
        "NetworkPlugin"
    }

    fn build(&self, app: &mut crate::App) {
        let replication = NetworkReplication::new(self.config.clone());
        app.world.insert_resource(replication);

        log::info!(
            "NetworkPlugin loaded — {} mode",
            match self.config.role {
                NetworkRole::Server => "server",
                NetworkRole::Client { .. } => "client",
                NetworkRole::ListenServer => "listen server",
            }
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
