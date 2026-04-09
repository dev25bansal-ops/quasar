//! Networking Demo for Quasar Game Engine
//!
//! Demonstrates the lobby system with both server and client functionality:
//! - Starts a lobby server
//! - Creates game sessions
//! - Players join/leave sessions
//! - Session discovery
//!
//! # Usage
//!
//! ```sh
//! cargo run -p networking_demo
//! ```

use quasar_lobby::{
    LobbyClient, LobbyError, PlayerId, SessionConfig, SessionFilters, SessionId, SessionState,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;

type SessionStore = Arc<RwLock<HashMap<SessionId, quasar_lobby::Session>>>;

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

async fn run_server(sessions: SessionStore) -> std::net::SocketAddr {
    let addr = "127.0.0.1:0";
    let listener = TcpListener::bind(addr).await.expect("Failed to bind");
    let bound_addr = listener.local_addr().expect("Failed to get address");
    
    log::info!("Lobby server listening on {}", bound_addr);
    
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    log::debug!("Connection from {}", peer_addr);
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
    
    bound_addr
}

async fn handle_connection(stream: TcpStream, sessions: SessionStore) {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).await.is_err() || request_line.is_empty() {
        return;
    }
    request_line = request_line.trim().to_string();
    
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 3 {
        send_error(&mut writer, 400, "Bad Request").await;
        return;
    }
    
    let method = parts[0];
    let path = parts[1];
    let path_without_query = path.split('?').next().unwrap_or(path);
    
    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await.is_err() {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.to_lowercase(), value.trim().to_string());
        }
    }
    
    let content_length: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    
    let body_str = if content_length > 0 {
        let mut body_buf = vec![0u8; content_length];
        if reader.read_exact(&mut body_buf).await.is_err() {
            String::new()
        } else {
            String::from_utf8_lossy(&body_buf).to_string()
        }
    } else {
        String::new()
    };
    
    let response = match (method, path_without_query) {
        ("GET", path) if path.starts_with("/api/sessions/") => {
            let session_id_str = path.trim_start_matches("/api/sessions/");
            handle_get_session(session_id_str, sessions.clone()).await
        }
        ("GET", "/api/sessions") => handle_list_sessions(sessions.clone()).await,
        ("POST", "/api/sessions") => {
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
                LobbyError::Server { code, message } => (code, message),
                _ => (500, "Internal server error".to_string()),
            };
            send_error(&mut writer, code, &msg).await;
        }
    }
}

async fn handle_get_session(session_id_str: &str, sessions: SessionStore) -> Result<String, LobbyError> {
    let session_id: u64 = u64::from_str_radix(session_id_str, 16)
        .map_err(|_| LobbyError::SessionNotFound(SessionId(0)))?;
    let store = sessions.read().await;
    let session = store
        .get(&SessionId(session_id))
        .ok_or(LobbyError::SessionNotFound(SessionId(session_id)))?;
    serde_json::to_string(session).map_err(|e| LobbyError::Serialization(e.to_string()))
}

async fn handle_list_sessions(sessions: SessionStore) -> Result<String, LobbyError> {
    let store = sessions.read().await;
    let list: Vec<&quasar_lobby::Session> = store.values().collect();
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
    let player = quasar_lobby::PlayerInfo {
        id: request.player_id.clone(),
        name: format!("Player-{}", request.player_id.0.chars().take(6).collect::<String>()),
        team: None,
        is_ready: false,
        metadata: HashMap::new(),
    };
    
    let session = quasar_lobby::Session {
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
        session: quasar_lobby::Session,
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
    
    let session_id: u64 = u64::from_str_radix(session_id_str, 16)
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
    
    let player = quasar_lobby::PlayerInfo {
        id: request.player_id.clone(),
        name: format!("Player-{}", request.player_id.0.chars().take(6).collect::<String>()),
        team: None,
        is_ready: false,
        metadata: HashMap::new(),
    };
    
    session.players.push(player);
    session.player_count += 1;
    
    let join_info = quasar_lobby::JoinInfo {
        session: session.clone(),
        connection_token: format!("token-{}", session_id),
        server_address: session.server_address.clone().unwrap_or_default(),
        player_id: request.player_id,
    };
    
    #[derive(serde::Serialize)]
    struct Response {
        join_info: quasar_lobby::JoinInfo,
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
    
    let session_id: u64 = u64::from_str_radix(session_id_str, 16)
        .map_err(|_| LobbyError::SessionNotFound(SessionId(0)))?;
    
    let mut store = sessions.write().await;
    let session = store
        .get_mut(&SessionId(session_id))
        .ok_or(LobbyError::SessionNotFound(SessionId(session_id)))?;
    
    session.players.retain(|p| p.id != request.player_id);
    session.player_count = session.players.len() as u32;
    
    Ok("null".to_string())
}

async fn handle_destroy_session(
    session_id_str: &str,
    sessions: SessionStore,
) -> Result<String, LobbyError> {
    let session_id: u64 = u64::from_str_radix(session_id_str, 16)
        .map_err(|_| LobbyError::SessionNotFound(SessionId(0)))?;
    
    let mut store = sessions.write().await;
    store
        .remove(&SessionId(session_id))
        .ok_or(LobbyError::SessionNotFound(SessionId(session_id)))?;
    
    Ok("null".to_string())
}

async fn send_json(stream: &mut tokio::net::tcp::OwnedWriteHalf, status: u16, body: &str) {
    let response = format!(
        "HTTP/1.1 {} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        status,
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.flush().await;
}

async fn send_error(stream: &mut tokio::net::tcp::OwnedWriteHalf, status: u16, message: &str) {
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

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    
    println!("=== Quasar Networking Demo ===\n");
    
    let sessions: SessionStore = Arc::new(RwLock::new(HashMap::new()));
    
    let server_addr = run_server(sessions.clone()).await;
    let server_url = format!("http://{}", server_addr);
    
    println!("Server started at {}", server_url);
    
    let client = LobbyClient::new(&server_url);
    
    println!("\n--- Player 1: Creating a session ---");
    
    let config = SessionConfig {
        name: "Epic Deathmatch".to_string(),
        max_players: 4,
        game_mode: "deathmatch".to_string(),
        region: Some("us-west".to_string()),
        is_public: true,
        ..Default::default()
    };
    
    let create_result = client.create_session(config.clone()).await;
    let (session, _join_info) = match create_result {
        Ok((session, join_info)) => {
            println!("Session created: {}", session.id);
            println!("Session name: {}", session.config.name);
            println!("Max players: {}", session.config.max_players);
            println!("Connection token: {}", join_info.connection_token);
            println!("Player ID: {}", join_info.player_id);
            (session, join_info)
        }
        Err(e) => {
            println!("Failed to create session: {}", e);
            return;
        }
    };
    
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    println!("\n--- Player 2: Finding and joining the session ---");
    let player2_id = PlayerId::new();
    println!("Player 2 ID: {}", player2_id);
    
    let filters = SessionFilters {
        game_mode: Some("deathmatch".to_string()),
        region: Some("us-west".to_string()),
        ..Default::default()
    };
    
    let sessions_found = client.find_sessions(filters.clone()).await;
    match sessions_found {
        Ok(list) => {
            println!("Found {} session(s)", list.len());
            for s in &list {
                println!("  - {} ({}/{} players)", s.config.name, s.player_count, s.config.max_players);
            }
        }
        Err(e) => {
            println!("Failed to find sessions: {}", e);
        }
    }
    
    let join_result = client.join_session(session.id, player2_id.clone(), None).await;
    match join_result {
        Ok(join_info) => {
            println!("Player 2 joined session: {}", join_info.session.id);
            println!("Players now: {}/{}", join_info.session.player_count, join_info.session.config.max_players);
            for player in &join_info.session.players {
                println!("  - {} ({})", player.name, player.id);
            }
        }
        Err(e) => {
            println!("Failed to join session: {}", e);
        }
    }
    
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    println!("\n--- Checking session state ---");
    let session_info = client.get_session(session.id).await;
    match session_info {
        Ok(s) => {
            println!("Session: {}", s.config.name);
            println!("State: {:?}", s.state);
            println!("Players: {}", s.player_count);
        }
        Err(e) => {
            println!("Failed to get session: {}", e);
        }
    }
    
    println!("\n--- Player 2: Leaving the session ---");
    let leave_result = client.leave_session(session.id, player2_id.clone()).await;
    match leave_result {
        Ok(_) => {
            println!("Player 2 left the session");
        }
        Err(e) => {
            println!("Failed to leave session: {}", e);
        }
    }
    
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    println!("\n--- Final session state ---");
    let final_session = client.get_session(session.id).await;
    match final_session {
        Ok(s) => {
            println!("Session: {}", s.config.name);
            println!("Players remaining: {}", s.player_count);
        }
        Err(e) => {
            println!("Failed to get session: {}", e);
        }
    }
    
    println!("\n--- Destroying the session ---");
    let destroy_result = client.destroy_session(session.id).await;
    match destroy_result {
        Ok(_) => {
            println!("Session destroyed");
        }
        Err(e) => {
            println!("Failed to destroy session: {}", e);
        }
    }
    
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    println!("\n--- Verifying session is gone ---");
    let gone = client.get_session(session.id).await;
    match gone {
        Ok(_) => {
            println!("ERROR: Session still exists!");
        }
        Err(_) => {
            println!("Session successfully removed");
        }
    }
    
    println!("\n=== Demo Complete ===");
}
