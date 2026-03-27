//! Lobby and matchmaking system for Quasar game engine.
//!
//! Provides a REST/WebSocket client for session management:
//! - Create game sessions with custom settings
//! - Find available sessions by filters
//! - Join sessions and receive connection details
//!
//! # Example
//!
//! ```ignore
//! use quasar_lobby::{LobbyClient, SessionConfig, SessionId, PlayerId};
//!
//! async fn run() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = LobbyClient::new("https://lobby.example.com");
//!
//!     // Create a session
//!     let config = SessionConfig {
//!         name: "My Game".to_string(),
//!         max_players: 4,
//!         game_mode: "deathmatch".to_string(),
//!         ..Default::default()
//!     };
//!     let session = client.create_session(config).await?;
//!
//!     // Find sessions
//!     let sessions = client.find_sessions(Default::default()).await?;
//!
//!     // Join a session
//!     let player_id = PlayerId::new();
//!     let join_info = client.join_session(session.id, player_id, None).await?;
//!
//!     Ok(())
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::str::FromStr;

mod client;
mod protocol;
#[cfg(feature = "server")]
mod server;

pub use client::*;
pub use protocol::*;
#[cfg(feature = "server")]
pub use server::*;

/// Unique identifier for a game session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub u64);

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

impl FromStr for SessionId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        u64::from_str_radix(s, 16)
            .map(SessionId)
            .map_err(|e| format!("Invalid session ID: {}", e))
    }
}

/// Unique identifier for a player.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub String);

impl PlayerId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl Default for PlayerId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for PlayerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Configuration for creating a new game session.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionConfig {
    /// Human-readable name for the session.
    pub name: String,
    /// Maximum number of players allowed.
    pub max_players: u32,
    /// Game mode identifier (e.g., "deathmatch", "team-battle").
    pub game_mode: String,
    /// Region hint for server selection.
    pub region: Option<String>,
    /// Custom key-value metadata.
    pub metadata: HashMap<String, String>,
    /// Whether the session is publicly listed.
    pub is_public: bool,
    /// Password for private sessions (optional).
    pub password: Option<String>,
}

/// A game session that players can join.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier.
    pub id: SessionId,
    /// Session configuration.
    pub config: SessionConfig,
    /// Current number of players in the session.
    pub player_count: u32,
    /// List of connected players.
    pub players: Vec<PlayerInfo>,
    /// Session state.
    pub state: SessionState,
    /// Server connection endpoint (host:port).
    pub server_address: Option<String>,
    /// When the session was created.
    pub created_at: u64,
}

/// Information about a connected player.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerInfo {
    /// Player identifier.
    pub id: PlayerId,
    /// Display name.
    pub name: String,
    /// Player's team (if applicable).
    pub team: Option<u32>,
    /// Whether the player is ready.
    pub is_ready: bool,
    /// Custom player data.
    pub metadata: HashMap<String, String>,
}

/// State of a game session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    /// Waiting for players to join.
    Lobby,
    /// Game is starting.
    Starting,
    /// Game is in progress.
    InProgress,
    /// Game has ended.
    Ended,
}

/// Filters for searching sessions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionFilters {
    /// Filter by game mode.
    pub game_mode: Option<String>,
    /// Filter by region.
    pub region: Option<String>,
    /// Filter by minimum player count.
    pub min_players: Option<u32>,
    /// Filter by maximum player count.
    pub max_players: Option<u32>,
    /// Filter by session state.
    pub state: Option<SessionState>,
    /// Custom filter key-values.
    pub metadata: HashMap<String, String>,
    /// Maximum number of results to return.
    pub limit: Option<u32>,
}

/// Information returned after successfully joining a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinInfo {
    /// The session that was joined.
    pub session: Session,
    /// Connection token for authentication.
    pub connection_token: String,
    /// Server address to connect to.
    pub server_address: String,
    /// Player ID assigned to the joiner.
    pub player_id: PlayerId,
}

/// Errors that can occur in lobby operations.
#[derive(Debug)]
pub enum LobbyError {
    /// Network error during request.
    Network(String),
    /// Server returned an error response.
    Server { code: u16, message: String },
    /// Session not found.
    SessionNotFound(SessionId),
    /// Session is full.
    SessionFull(SessionId),
    /// Invalid password provided.
    InvalidPassword,
    /// Player is already in a session.
    AlreadyInSession,
    /// Serialization error.
    Serialization(String),
}

impl fmt::Display for LobbyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LobbyError::Network(msg) => write!(f, "Network error: {}", msg),
            LobbyError::Server { code, message } => write!(f, "Server error {}: {}", code, message),
            LobbyError::SessionNotFound(id) => write!(f, "Session not found: {}", id),
            LobbyError::SessionFull(id) => write!(f, "Session is full: {}", id),
            LobbyError::InvalidPassword => write!(f, "Invalid password"),
            LobbyError::AlreadyInSession => write!(f, "Player already in a session"),
            LobbyError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
        }
    }
}

impl Error for LobbyError {}
