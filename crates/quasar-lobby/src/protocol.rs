//! Protocol types for lobby communication.

use serde::{Deserialize, Serialize};

/// Request to create a new session.
#[derive(Debug, Serialize)]
pub struct CreateSessionRequest {
    pub config: super::SessionConfig,
    pub player_id: super::PlayerId,
}

/// Response after creating a session.
#[derive(Debug, Deserialize)]
pub struct CreateSessionResponse {
    pub session: super::Session,
    pub connection_token: String,
    pub server_address: String,
}

/// Request to join a session.
#[derive(Debug, Serialize)]
pub struct JoinSessionRequest {
    pub session_id: super::SessionId,
    pub player_id: super::PlayerId,
    pub password: Option<String>,
}

/// Response after joining a session.
#[derive(Debug, Deserialize)]
pub struct JoinSessionResponse {
    pub join_info: super::JoinInfo,
}

/// Request to leave a session.
#[derive(Debug, Serialize)]
pub struct LeaveSessionRequest {
    pub session_id: super::SessionId,
    pub player_id: super::PlayerId,
}

/// Request to update session state.
#[derive(Debug, Serialize)]
pub struct UpdateSessionRequest {
    pub session_id: super::SessionId,
    pub player_id: super::PlayerId,
    pub state: super::SessionState,
}

/// Request to update player state.
#[derive(Debug, Serialize)]
pub struct UpdatePlayerRequest {
    pub session_id: super::SessionId,
    pub player_id: super::PlayerId,
    pub is_ready: bool,
    pub team: Option<u32>,
    pub metadata: std::collections::HashMap<String, String>,
}

/// WebSocket message for real-time updates.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsMessage {
    /// Client subscribes to session updates.
    Subscribe { session_id: super::SessionId },
    /// Client unsubscribes from session updates.
    Unsubscribe { session_id: super::SessionId },
    /// Server pushes session update.
    SessionUpdate { session: super::Session },
    /// Server notifies player joined.
    PlayerJoined {
        session_id: super::SessionId,
        player: super::PlayerInfo,
    },
    /// Server notifies player left.
    PlayerLeft {
        session_id: super::SessionId,
        player_id: super::PlayerId,
    },
    /// Server notifies session state changed.
    StateChanged {
        session_id: super::SessionId,
        state: super::SessionState,
    },
    /// Server notifies session was destroyed.
    SessionDestroyed { session_id: super::SessionId },
    /// Heartbeat to keep connection alive.
    Heartbeat,
    /// Heartbeat acknowledgment.
    HeartbeatAck,
}
