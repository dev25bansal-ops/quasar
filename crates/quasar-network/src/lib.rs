//! Networking transport and protocol types for Quasar Engine.
//!
//! This crate provides transport-agnostic networking primitives:
//! - Transport trait and implementations (UDP, QUIC)
//! - Protocol types (NetworkMessage, NetworkPayload)
//! - Connection management types
//! - Authentication and session management
//!
//! For ECS integration, see `quasar-core::network`.

pub mod auth;

use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};

use serde::{Deserialize, Serialize};

pub const DEFAULT_PORT: u16 = 7777;
pub const MAX_CLIENTS: usize = 32;
pub const TICK_RATE: u32 = 60;
pub const MTU_SIZE: usize = 1400;
pub const MAX_MESSAGE_SIZE: usize = 1024 * 1024;
pub const MIN_MESSAGE_SIZE: usize = 8;
pub const MAX_ENTITIES_PER_SNAPSHOT: usize = 1024;
pub const MAX_COMPONENTS_PER_ENTITY: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClientId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NetworkEntityId(pub u64);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NetworkRole {
    Server,
    Client { server_addr: SocketAddr },
    ListenServer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TransportType {
    #[default]
    Udp,
    #[cfg(feature = "quinn-transport")]
    Quic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendChannel {
    Unreliable,
    Reliable,
    Bulk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuicChannel {
    Unreliable,
    Reliable,
    BulkTransfer,
}

#[derive(Debug, Clone)]
pub enum QuicEvent {
    Connected(SocketAddr),
    Disconnected(SocketAddr, String),
    Data {
        from: SocketAddr,
        channel: QuicChannel,
        payload: Vec<u8>,
    },
}

pub trait QuicTransportBackend {
    fn connect(&mut self, addr: SocketAddr) -> Result<(), NetworkError>;
    fn listen(&mut self, addr: SocketAddr) -> Result<(), NetworkError>;
    fn poll(&mut self) -> Vec<QuicEvent>;
    fn send(&mut self, addr: SocketAddr, channel: QuicChannel, data: &[u8]) -> Result<(), NetworkError>;
    fn peer_count(&self) -> usize;
    fn disconnect(&mut self, addr: SocketAddr);
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ConnectionMetrics {
    pub rtt_ms: f32,
    pub packet_loss: f32,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub last_update: Option<std::time::Instant>,
}

pub trait Transport: Send + Sync {
    fn connect(&mut self, addr: SocketAddr) -> Result<(), NetworkError>;
    fn listen(&mut self, addr: SocketAddr) -> Result<(), NetworkError>;
    fn poll(&mut self) -> Vec<TransportEvent>;
    fn send(
        &mut self,
        addr: SocketAddr,
        channel: SendChannel,
        data: &[u8],
    ) -> Result<(), NetworkError>;
    fn peer_count(&self) -> usize;
    fn disconnect(&mut self, addr: SocketAddr);
    fn metrics(&self, addr: SocketAddr) -> Option<ConnectionMetrics>;
    fn local_addr(&self) -> Result<SocketAddr, NetworkError>;
}

#[derive(Debug, Clone)]
pub enum TransportEvent {
    Connected(SocketAddr),
    Disconnected(SocketAddr, String),
    Data {
        from: SocketAddr,
        channel: SendChannel,
        payload: Vec<u8>,
    },
}

#[derive(Debug, Clone, Default)]
pub struct NetworkMetricsResource {
    pub local_rtt_ms: f32,
    pub local_packet_loss: f32,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub peer_count: usize,
    pub transport_type: TransportType,
    pub fallback_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub role: NetworkRole,
    pub port: u16,
    pub max_clients: usize,
    pub tick_rate: u32,
    pub interpolation_delay_ms: u32,
    pub rollback_frame_count: u32,
    pub transport: TransportType,
    pub auto_fallback: bool,
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
            transport: TransportType::default(),
            auto_fallback: true,
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
    ConnectionRequest { client_id: ClientId },
    ConnectionAccepted { client_id: ClientId },
    ConnectionDenied { reason: String },
    Disconnect { client_id: ClientId },
    EntitySpawn { entity_id: NetworkEntityId, components: Vec<ComponentData> },
    EntityDespawn { entity_id: NetworkEntityId },
    EntityUpdate { entity_id: NetworkEntityId, components: Vec<ComponentData> },
    EntityTransform {
        entity_id: NetworkEntityId,
        position: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
    },
    Input { client_id: ClientId, inputs: Vec<InputData> },
    StateSnapshot { frame: u64, entities: Vec<EntitySnapshot> },
    Rpc { entity_id: NetworkEntityId, method: String, args: Vec<u8> },
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
        matches!(self.config.role, NetworkRole::Server | NetworkRole::ListenServer)
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

impl From<bincode::error::EncodeError> for NetworkError {
    fn from(e: bincode::error::EncodeError) -> Self {
        NetworkError(e.to_string())
    }
}

impl From<bincode::error::DecodeError> for NetworkError {
    fn from(e: bincode::error::DecodeError) -> Self {
        NetworkError(e.to_string())
    }
}

pub struct UdpTransport {
    socket: UdpSocket,
    peers: HashMap<SocketAddr, PeerInfo>,
}

#[allow(dead_code)]
struct PeerInfo {
    last_sequence: u64,
    send_channel: VecDeque<(u64, Vec<u8>)>,
}

use std::collections::VecDeque;

impl UdpTransport {
    pub fn bind(addr: SocketAddr) -> Result<Self, NetworkError> {
        let socket = UdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;
        Ok(Self {
            socket,
            peers: HashMap::new(),
        })
    }
}

impl Transport for UdpTransport {
    fn connect(&mut self, addr: SocketAddr) -> Result<(), NetworkError> {
        self.peers.entry(addr).or_insert_with(|| PeerInfo {
            last_sequence: 0,
            send_channel: VecDeque::new(),
        });
        Ok(())
    }

    fn listen(&mut self, _addr: SocketAddr) -> Result<(), NetworkError> {
        Ok(())
    }

    fn poll(&mut self) -> Vec<TransportEvent> {
        let mut events = Vec::new();
        let mut buf = [0u8; 65507];

        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((len, addr)) => {
                    let payload = buf[..len].to_vec();
                    events.push(TransportEvent::Data {
                        from: addr,
                        channel: SendChannel::Unreliable,
                        payload,
                    });
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    log::warn!("UDP recv error: {}", e);
                    break;
                }
            }
        }

        events
    }

    fn send(&mut self, addr: SocketAddr, _channel: SendChannel, data: &[u8]) -> Result<(), NetworkError> {
        self.socket.send_to(data, addr)?;
        Ok(())
    }

    fn peer_count(&self) -> usize {
        self.peers.len()
    }

    fn disconnect(&mut self, addr: SocketAddr) {
        self.peers.remove(&addr);
    }

    fn metrics(&self, _addr: SocketAddr) -> Option<ConnectionMetrics> {
        None
    }

    fn local_addr(&self) -> Result<SocketAddr, NetworkError> {
        Ok(self.socket.local_addr()?)
    }
}

pub fn validate_message(message: &NetworkMessage) -> Result<(), NetworkError> {
    let config = bincode::config::standard();
    let encoded = bincode::serde::encode_to_vec(message, config)?;
    if encoded.len() < MIN_MESSAGE_SIZE {
        return Err(NetworkError(format!(
            "Message too small: {} < {}",
            encoded.len(), MIN_MESSAGE_SIZE
        )));
    }
    if encoded.len() > MAX_MESSAGE_SIZE {
        return Err(NetworkError(format!(
            "Message too large: {} > {}",
            encoded.len(), MAX_MESSAGE_SIZE
        )));
    }
    match &message.payload {
        NetworkPayload::StateSnapshot { entities, .. } => {
            if entities.len() > MAX_ENTITIES_PER_SNAPSHOT {
                return Err(NetworkError(format!(
                    "Snapshot has too many entities: {} > {}",
                    entities.len(), MAX_ENTITIES_PER_SNAPSHOT
                )));
            }
        }
        NetworkPayload::EntitySpawn { components, .. } |
        NetworkPayload::EntityUpdate { components, .. } => {
            if components.len() > MAX_COMPONENTS_PER_ENTITY {
                return Err(NetworkError(format!(
                    "Entity has too many components: {} > {}",
                    components.len(), MAX_COMPONENTS_PER_ENTITY
                )));
            }
        }
        NetworkPayload::Rpc { args, .. } => {
            if args.len() > MAX_MESSAGE_SIZE / 4 {
                return Err(NetworkError(format!(
                    "RPC args too large: {} bytes",
                    args.len()
                )));
            }
        }
        _ => {}
    }
    Ok(())
}

pub mod replication;

#[cfg(feature = "quinn-transport")]
pub mod quinn;

#[cfg(feature = "quinn-transport")]
pub use quinn::QuinnBackend;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_id_equality() {
        let id1 = ClientId(42);
        let id2 = ClientId(42);
        let id3 = ClientId(43);
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_network_entity_id_equality() {
        let id1 = NetworkEntityId(100);
        let id2 = NetworkEntityId(100);
        let id3 = NetworkEntityId(101);
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_network_config_default() {
        let config = NetworkConfig::default();
        assert_eq!(config.port, DEFAULT_PORT);
        assert_eq!(config.max_clients, MAX_CLIENTS);
        assert_eq!(config.tick_rate, TICK_RATE);
        assert!(config.auto_fallback);
    }

    #[test]
    fn test_network_role_serialization() {
        let role = NetworkRole::Server;
        let json = serde_json::to_string(&role).expect("Serialization failed");
        let deserialized: NetworkRole = serde_json::from_str(&json).expect("Deserialization failed");
        assert_eq!(role, deserialized);
    }

    #[test]
    fn test_network_role_client() {
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let role = NetworkRole::Client { server_addr: addr };
        match role {
            NetworkRole::Client { server_addr } => assert_eq!(server_addr, addr),
            _ => panic!("Expected Client variant"),
        }
    }

    #[test]
    fn test_transport_type_default() {
        let transport = TransportType::default();
        assert_eq!(transport, TransportType::Udp);
    }

    #[test]
    fn test_send_channel_variants() {
        assert_eq!(SendChannel::Unreliable, SendChannel::Unreliable);
        assert_ne!(SendChannel::Unreliable, SendChannel::Reliable);
        assert_ne!(SendChannel::Reliable, SendChannel::Bulk);
    }

    #[test]
    fn test_connection_metrics_default() {
        let metrics = ConnectionMetrics::default();
        assert_eq!(metrics.rtt_ms, 0.0);
        assert_eq!(metrics.packet_loss, 0.0);
        assert_eq!(metrics.bytes_sent, 0);
        assert!(metrics.last_update.is_none());
    }

    #[test]
    fn test_network_metrics_default() {
        let metrics = NetworkMetricsResource::default();
        assert_eq!(metrics.local_rtt_ms, 0.0);
        assert_eq!(metrics.peer_count, 0);
        assert!(!metrics.fallback_active);
    }

    #[test]
    fn test_network_state_new() {
        let config = NetworkConfig::default();
        let state = NetworkState::new(config);
        assert_eq!(state.next_entity_id, 1);
        assert_eq!(state.next_client_id, 1);
        assert_eq!(state.frame_number, 0);
        assert!(state.clients.is_empty());
    }

    #[test]
    fn test_network_state_is_server() {
        let config = NetworkConfig::default();
        let state = NetworkState::new(config);
        assert!(state.is_server());
        assert!(!state.is_client());
    }

    #[test]
    fn test_network_state_is_client() {
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let config = NetworkConfig {
            role: NetworkRole::Client { server_addr: addr },
            ..Default::default()
        };
        let state = NetworkState::new(config);
        assert!(!state.is_server());
        assert!(state.is_client());
    }

    #[test]
    fn test_network_client_new() {
        let addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let client = NetworkClient::new(ClientId(1), addr);
        assert_eq!(client.id, ClientId(1));
        assert_eq!(client.addr, addr);
        assert!(client.connected);
        assert_eq!(client.rtt_ms, 0.0);
    }

    #[test]
    fn test_network_error_display() {
        let err = NetworkError("Test error".to_string());
        assert_eq!(format!("{}", err), "Test error");
    }

    #[test]
    fn test_network_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let net_err: NetworkError = io_err.into();
        assert!(net_err.0.contains("file not found"));
    }

    #[test]
    fn test_network_message_serialization() {
        let msg = NetworkMessage {
            sequence: 42,
            timestamp: 1234567890,
            payload: NetworkPayload::Input {
                client_id: ClientId(1),
                inputs: vec![InputData {
                    input_type: InputType::MoveForward,
                    value: 1.0,
                }],
            },
        };
        
        let config = bincode::config::standard();
        let encoded = bincode::serde::encode_to_vec(&msg, config).expect("Encoding failed");
        let (decoded, _): (NetworkMessage, _) = bincode::serde::decode_from_slice(&encoded, config).expect("Decoding failed");
        
        assert_eq!(msg.sequence, decoded.sequence);
        assert_eq!(msg.timestamp, decoded.timestamp);
    }

    #[test]
    fn test_entity_snapshot_serialization() {
        let snapshot = EntitySnapshot {
            entity_id: NetworkEntityId(42),
            position: [1.0, 2.0, 3.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
            frame: 100,
        };
        
        let config = bincode::config::standard();
        let encoded = bincode::serde::encode_to_vec(&snapshot, config).expect("Encoding failed");
        let (decoded, _): (EntitySnapshot, _) = bincode::serde::decode_from_slice(&encoded, config).expect("Decoding failed");
        
        assert_eq!(snapshot.entity_id, decoded.entity_id);
        assert_eq!(snapshot.position, decoded.position);
    }

    #[test]
    fn test_input_type_custom() {
        let input = InputType::Custom("Jump".to_string());
        match input {
            InputType::Custom(name) => assert_eq!(name, "Jump"),
            _ => panic!("Expected Custom variant"),
        }
    }

    #[test]
    fn test_component_data_creation() {
        let comp = ComponentData {
            type_name: "Transform".to_string(),
            data: vec![1, 2, 3, 4],
        };
        assert_eq!(comp.type_name, "Transform");
        assert_eq!(comp.data.len(), 4);
    }

    #[test]
    fn test_network_payload_variants() {
        let payloads = vec![
            NetworkPayload::ConnectionRequest { client_id: ClientId(1) },
            NetworkPayload::ConnectionAccepted { client_id: ClientId(1) },
            NetworkPayload::ConnectionDenied { reason: "Full".to_string() },
            NetworkPayload::Disconnect { client_id: ClientId(1) },
            NetworkPayload::EntitySpawn { entity_id: NetworkEntityId(1), components: vec![] },
            NetworkPayload::Rpc {
                entity_id: NetworkEntityId(1),
                method: "test".to_string(),
                args: vec![],
            },
        ];
        
        for payload in payloads {
            let msg = NetworkMessage {
                sequence: 1,
                timestamp: 0,
                payload,
            };
            let config = bincode::config::standard();
            let encoded = bincode::serde::encode_to_vec(&msg, config).expect("Encoding failed");
            assert!(!encoded.is_empty());
        }
    }

    #[test]
    fn test_validate_message_rejects_small() {
        let msg = NetworkMessage {
            sequence: 0,
            timestamp: 0,
            payload: NetworkPayload::Input { client_id: ClientId(0), inputs: vec![] },
        };
        let result = validate_message(&msg);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_message_accepts_valid() {
        let msg = NetworkMessage {
            sequence: 1,
            timestamp: 12345,
            payload: NetworkPayload::StateSnapshot {
                frame: 1,
                entities: vec![EntitySnapshot {
                    entity_id: NetworkEntityId(1),
                    position: [0.0, 0.0, 0.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0, 1.0, 1.0],
                    frame: 1,
                }],
            },
        };
        let result = validate_message(&msg);
        assert!(result.is_ok());
    }
}
