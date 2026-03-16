//! Reference Lobby Server Implementation
//!
//! A simple HTTP/WebSocket server for managing game sessions.
//!
//! # Usage
//!
//! ```sh
//! cargo run --example lobby_server
//! ```
//!
//! Server listens on `http://localhost:8080` by default.

use quasar_lobby::{
    JoinInfo, LobbyError, PlayerId, PlayerInfo, Session, SessionConfig,
    SessionId, SessionState,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::sync::RwLock;

/// In-memory session storage.
type SessionStore = Arc<RwLock<HashMap<SessionId, Session>>>;

/// Generate a unique session ID.
fn generate_session_id() -> SessionId {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    SessionId(timestamp.wrapping_add(rand_u64()))
}

fn rand_u64() -> u64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let state = RandomState::new();
    let mut hasher = state.build_hasher();
    hasher.write_u64(std::process::id() as u64);
    hasher.write_u64(chrono_timestamp());
    hasher.finish()
}

fn chrono_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Handle an incoming HTTP request.
async fn handle_connection(stream: TcpStream, sessions: SessionStore) {
    let (reader, mut writer) = stream.into_split();
    let reader = BufReader::new(reader);
    let mut lines = reader.lines();

    // Read request line
    let request_line = match lines.next_line().await {
        Ok(Some(line)) => line,
        _ => return,
    };

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 3 {
        send_error(&mut writer, 400, "Bad Request").await;
        return;
    }

    let method = parts[0];
    let path = parts[1];

    // Read headers
    let mut headers = HashMap::new();
    loop {
        let line = match lines.next_line().await {
            Ok(Some(l)) => l,
            _ => break,
        };
        if line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.to_lowercase(), value.trim().to_string());
        }
    }

    // Read body if present
    let content_length: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let body = vec![0u8; content_length];
    if content_length > 0 {
        // Body already read via lines; content-length handling simplified
    }
    let body_str = String::from_utf8_lossy(&body);

    // Route request
    let response = match (method, path) {
        ("GET", path) if path.starts_with("/api/sessions/") => {
            let session_id_str = path.trim_start_matches("/api/sessions/");
            handle_get_session(session_id_str, sessions.clone()).await
        }
        ("GET", "/api/sessions") => handle_list_sessions(sessions.clone(), "").await,
        ("POST", path) if path.starts_with("/api/sessions") && !path.contains('/') => {
            handle_create_session(&body_str, sessions.clone()).await
        }
        ("POST", path) if path.starts_with("/api/sessions/") && path.ends_with("/join") => {
            let path = path.trim_start_matches("/api/sessions/");
            let session_id_str = path.trim_end_matches("/join");
            handle_join_session(session_id_str, &body_str, sessions.clone()).await
        }
        ("POST", path) if path.starts_with("/api/sessions/") && path.ends_with("/leave") => {
            let path = path.trim_start_matches("/api/sessions/");
            let session_id_str = path.trim_end_matches("/leave");
            handle_leave_session(session_id_str, &body_str, sessions.clone()).await
        }
        ("DELETE", path) if path.starts_with("/api/sessions/") => {
            let session_id_str = path.trim_start_matches("/api/sessions/");
            handle_destroy_session(session_id_str, sessions.clone()).await
        }
        ("GET", "/health") => Ok("OK".to_string()),
        _ => Err(LobbyError::Server {
            code: 404,
            message: "Not Found".to_string(),
        }),
    };

    match response {
        Ok(json) => send_json(&mut writer, 200, &json).await,
        Err(e) => {
            let (code, msg): (u16, String) = match e {
                LobbyError::SessionNotFound(_) => (404, "Session not found".to_string()),
                LobbyError::SessionFull(_) => (409, "Session is full".to_string()),
                LobbyError::InvalidPassword => (401, "Invalid password".to_string()),
                LobbyError::AlreadyInSession => (409, "Already in session".to_string()),
                LobbyError::Serialization(_) => (400, "Invalid request body".to_string()),
                LobbyError::Server { code, message } => (code, message.to_string()),
                _ => (500, "Internal server error".to_string()),
            };
            send_error(&mut writer, code, &msg).await;
        }
    }
}

async fn handle_get_session(session_id_str: &str, sessions: SessionStore) -> Result<String, LobbyError> {
    let session_id: u64 = session_id_str
        .parse()
        .map_err(|_| LobbyError::SessionNotFound(SessionId(0)))?;
    let store = sessions.read().await;
    let session = store
        .get(&SessionId(session_id))
        .ok_or(LobbyError::SessionNotFound(SessionId(session_id)))?;
    serde_json::to_string(session).map_err(|e| LobbyError::Serialization(e.to_string()))
}

async fn handle_list_sessions(sessions: SessionStore, _query: &str) -> Result<String, LobbyError> {
    let store = sessions.read().await;
    let list: Vec<&Session> = store.values().collect();
    serde_json::to_string(&list).map_err(|e| LobbyError::Serialization(e.to_string()))
}

async fn handle_create_session(body: &str, sessions: SessionStore) -> Result<String, LobbyError> {
    #[derive(serde::Deserialize)]
    struct Request {
        config: SessionConfig,
        player_id: PlayerId,
    }

    let request: Request =
        serde_json::from_str(body).map_err(|e| LobbyError::Serialization(e.to_string()))?;

    let session_id = generate_session_id();
    let player = PlayerInfo {
        id: request.player_id.clone(),
        name: format!("Player-{}", request.player_id.0.chars().take(6).collect::<String>()),
        team: None,
        is_ready: false,
        metadata: HashMap::new(),
    };

    let session = Session {
        id: session_id,
        config: request.config,
        player_count: 1,
        players: vec![player],
        state: SessionState::Lobby,
        server_address: Some("127.0.0.1:7777".to_string()),
        created_at: chrono_timestamp(),
    };

    #[derive(serde::Serialize)]
    struct Response {
        session: Session,
        connection_token: String,
        server_address: String,
    }

    let response = Response {
        server_address: session.server_address.clone().unwrap_or_default(),
        connection_token: format!("token-{}", session_id.0),
        session: session.clone(),
    };

    let mut store = sessions.write().await;
    store.insert(session_id, session);

    serde_json::to_string(&response).map_err(|e| LobbyError::Serialization(e.to_string()))
}

async fn handle_join_session(
    session_id_str: &str,
    body: &str,
    sessions: SessionStore,
) -> Result<String, LobbyError> {
    #[derive(serde::Deserialize)]
    struct Request {
        player_id: PlayerId,
        password: Option<String>,
    }

    let request: Request =
        serde_json::from_str(body).map_err(|e| LobbyError::Serialization(e.to_string()))?;

    let session_id: u64 = session_id_str
        .parse()
        .map_err(|_| LobbyError::SessionNotFound(SessionId(0)))?;

    let mut store = sessions.write().await;
    let session = store
        .get_mut(&SessionId(session_id))
        .ok_or(LobbyError::SessionNotFound(SessionId(session_id)))?;

    if session.player_count >= session.config.max_players {
        return Err(LobbyError::SessionFull(SessionId(session_id)));
    }

    if let Some(ref required) = session.config.password {
        if request.password.as_ref() != Some(required) {
            return Err(LobbyError::InvalidPassword);
        }
    }

    let player = PlayerInfo {
        id: request.player_id.clone(),
        name: format!("Player-{}", request.player_id.0.chars().take(6).collect::<String>()),
        team: None,
        is_ready: false,
        metadata: HashMap::new(),
    };

    session.players.push(player);
    session.player_count += 1;

    let join_info = JoinInfo {
        session: session.clone(),
        connection_token: format!("token-{}", session_id),
        server_address: session.server_address.clone().unwrap_or_default(),
        player_id: request.player_id,
    };

    #[derive(serde::Serialize)]
    struct Response {
        join_info: JoinInfo,
    }

    serde_json::to_string(&Response { join_info })
        .map_err(|e| LobbyError::Serialization(e.to_string()))
}

async fn handle_leave_session(
    session_id_str: &str,
    body: &str,
    sessions: SessionStore,
) -> Result<String, LobbyError> {
    #[derive(serde::Deserialize)]
    struct Request {
        player_id: PlayerId,
    }

    let request: Request =
        serde_json::from_str(body).map_err(|e| LobbyError::Serialization(e.to_string()))?;

    let session_id: u64 = session_id_str
        .parse()
        .map_err(|_| LobbyError::SessionNotFound(SessionId(0)))?;

    let mut store = sessions.write().await;
    let session = store
        .get_mut(&SessionId(session_id))
        .ok_or(LobbyError::SessionNotFound(SessionId(session_id)))?;

    session.players.retain(|p| p.id != request.player_id);
    session.player_count = session.players.len() as u32;

    Ok("{}".to_string())
}

async fn handle_destroy_session(
    session_id_str: &str,
    sessions: SessionStore,
) -> Result<String, LobbyError> {
    let session_id: u64 = session_id_str
        .parse()
        .map_err(|_| LobbyError::SessionNotFound(SessionId(0)))?;

    let mut store = sessions.write().await;
    store
        .remove(&SessionId(session_id))
        .ok_or(LobbyError::SessionNotFound(SessionId(session_id)))?;

    Ok("{}".to_string())
}

async fn send_json(stream: &mut OwnedWriteHalf, status: u16, body: &str) {
    let response = format!(
        "HTTP/1.1 {} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        status,
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.flush().await;
}

async fn send_error(stream: &mut OwnedWriteHalf, status: u16, message: &str) {
    let body = serde_json::json!({ "error": message });
    let body_str = serde_json::to_string(&body).unwrap_or_default();
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        status,
        message,
        body_str.len(),
        body_str
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.flush().await;
}

fn main() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        env_logger::init();

        let addr = "127.0.0.1:8080";
        let listener = TcpListener::bind(addr).await.expect("Failed to bind");
        log::info!("Lobby server listening on {}", addr);

        let sessions: SessionStore = Arc::new(RwLock::new(HashMap::new()));

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    log::debug!("Connection from {}", addr);
                    let sessions = sessions.clone();
                    tokio::spawn(async move {
                        handle_connection(stream, sessions).await;
                    });
                }
                Err(e) => {
                    log::error!("Failed to accept connection: {}", e);
                }
            }
        }
    });
}
