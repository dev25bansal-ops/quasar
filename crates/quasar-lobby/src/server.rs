//! Lobby server implementation.
//!
//! Provides a TCP-based lobby server for session management:
//! - REST-like API for session CRUD operations
//! - WebSocket-style real-time updates
//! - Session matchmaking and player management

// ─── Security Constants (CRIT-003) ───────────────────────────────────────────
/// Maximum allowed size for incoming JSON lobby messages (64KB).
/// Prevents memory exhaustion from oversized payloads.
const MAX_REQUEST_SIZE: usize = 64 * 1024;

/// Maximum allowed nesting depth for JSON structures (32 levels).
/// Prevents stack overflow from deeply nested JSON bombs.
const MAX_JSON_DEPTH: usize = 32;

/// Maximum allowed length for individual string fields within JSON.
/// Prevents memory exhaustion from extremely long strings.
const MAX_STRING_LENGTH: usize = 4096;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, RwLock};
use tokio::time::timeout;

use crate::secret;
use crate::{
    JoinInfo, LobbyError, PlayerId, PlayerInfo, Session, SessionConfig, SessionId, SessionState,
};

// ─── Safe Deserialization (CRIT-003) ─────────────────────────────────────────

/// Error type for safe deserialization that distinguishes between
/// size violations, depth violations, and standard parse errors.
#[derive(Debug)]
enum SafeDeserializeError {
    /// The input exceeds the maximum allowed size.
    TooLarge(usize),
    /// The JSON structure exceeds the maximum allowed depth.
    TooDeep(usize),
    /// A string field exceeds the maximum allowed length.
    StringTooLong(usize),
    /// Standard serde deserialization error.
    Parse(serde_json::Error),
}

impl std::fmt::Display for SafeDeserializeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SafeDeserializeError::TooLarge(size) => {
                write!(f, "Request size {} bytes exceeds maximum {}", size, MAX_REQUEST_SIZE)
            }
            SafeDeserializeError::TooDeep(depth) => {
                write!(f, "JSON depth {} exceeds maximum {}", depth, MAX_JSON_DEPTH)
            }
            SafeDeserializeError::StringTooLong(len) => {
                write!(f, "String length {} exceeds maximum {}", len, MAX_STRING_LENGTH)
            }
            SafeDeserializeError::Parse(e) => {
                write!(f, "JSON parse error: {}", e)
            }
        }
    }
}

/// Safely deserialize JSON from untrusted input with size, depth, and string length limits.
///
/// This wrapper performs the following security checks:
/// 1. Validates input size does not exceed `MAX_REQUEST_SIZE`
/// 2. Checks JSON structural depth does not exceed `MAX_JSON_DEPTH`
/// 3. Validates all string fields do not exceed `MAX_STRING_LENGTH`
///
/// Returns `Err(SafeDeserializeError)` on any violation, allowing the caller
/// to return an appropriate HTTP 413 or 400 response.
fn safe_deserialize<'a, T: Deserialize<'a>>(input: &'a str) -> Result<T, SafeDeserializeError> {
    // Step 1: Validate total input size
    if input.len() > MAX_REQUEST_SIZE {
        return Err(SafeDeserializeError::TooLarge(input.len()));
    }

    // Step 2: Validate string lengths before deserialization
    validate_string_lengths(input)?;

    // Step 3: Validate JSON depth before deserialization
    validate_json_depth(input)?;

    // Step 4: Deserialize with all validations passed
    // serde_json::from_str has its own internal recursion limit (default 128),
    // but we enforce a stricter limit (32) via our pre-check above.
    let result = serde_json::from_str::<T>(input).map_err(SafeDeserializeError::Parse);

    result
}

/// Scan raw JSON input to verify no individual string value exceeds `MAX_STRING_LENGTH`.
///
/// This is a lightweight pre-check that scans for string literals in the raw JSON
/// without full parsing. It catches the most common memory exhaustion vectors.
fn validate_string_lengths(json: &str) -> Result<(), SafeDeserializeError> {
    let mut in_string = false;
    let mut escaped = false;
    let mut current_string_len = 0usize;

    for ch in json.chars() {
        if escaped {
            escaped = false;
            current_string_len += ch.len_utf8();
            continue;
        }

        if in_string {
            match ch {
                '\\' => {
                    escaped = true;
                    current_string_len += 1;
                }
                '"' => {
                    // End of string - validate length
                    if current_string_len > MAX_STRING_LENGTH {
                        return Err(SafeDeserializeError::StringTooLong(current_string_len));
                    }
                    in_string = false;
                    current_string_len = 0;
                }
                _ => {
                    current_string_len += ch.len_utf8();
                    if current_string_len > MAX_STRING_LENGTH {
                        return Err(SafeDeserializeError::StringTooLong(current_string_len));
                    }
                }
            }
        } else if ch == '"' {
            in_string = true;
            current_string_len = 0;
        }
    }

    // If we ended while still in a string, the JSON is malformed (will be caught by serde)
    Ok(())
}

/// Scan raw JSON input to verify the nesting depth does not exceed `MAX_JSON_DEPTH`.
///
/// This is a lightweight pre-check that counts opening/closing braces and brackets
/// to estimate the maximum nesting depth without full parsing.
fn validate_json_depth(json: &str) -> Result<(), SafeDeserializeError> {
    let mut current_depth = 0usize;
    let mut max_depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for ch in json.chars() {
        if escaped {
            escaped = false;
            continue;
        }

        if in_string {
            match ch {
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' | '[' => {
                current_depth += 1;
                if current_depth > max_depth {
                    max_depth = current_depth;
                }
                if current_depth > MAX_JSON_DEPTH {
                    return Err(SafeDeserializeError::TooDeep(current_depth));
                }
            }
            '}' | ']' => {
                if current_depth > 0 {
                    current_depth -= 1;
                }
            }
            _ => {}
        }
    }

    Ok(())
}

/// Generate an HTTP 413 Payload Too Large response.
fn http_413_payload_too_large() -> String {
    format!(
        "HTTP/1.1 413 Payload Too Large\r\nContent-Type: application/json\r\n\r\n{}",
        json_error("Payload too large")
    )
}

/// Generate an HTTP 400 Bad Request response for JSON errors.
fn http_400_bad_request(detail: &str) -> String {
    format!(
        "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\n\r\n{}",
        json_error(detail)
    )
}

/// Configuration for the lobby server.
#[derive(Debug, Clone)]
pub struct LobbyServerConfig {
    pub bind_address: String,
    pub port: u16,
    pub max_sessions: usize,
    pub session_timeout_secs: u64,
    pub heartbeat_interval_secs: u64,
    /// Secret key for token signing. If `None`, loaded from `QUASAR_LOBBY_SECRET` env var.
    pub secret_key: Option<Vec<u8>>,
}

impl Default for LobbyServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0".to_string(),
            port: 8080,
            max_sessions: 1000,
            session_timeout_secs: 3600,
            heartbeat_interval_secs: 30,
            secret_key: None, // Loaded from QUASAR_LOBBY_SECRET env var on resolve()
        }
    }
}

impl LobbyServerConfig {
    /// Resolve the secret key, loading from the environment if not explicitly configured.
    ///
    /// Panics in production if no secret is configured.
    pub fn resolve_secret(&self) -> Vec<u8> {
        self.secret_key
            .clone()
            .or_else(|| secret::load_lobby_secret())
            .unwrap_or_else(|| {
                panic!(
                    "CRITICAL: No lobby secret configured. Set the {} environment variable.",
                    secret::SECRET_ENV_VAR
                )
            })
    }
}

/// Internal session state.
#[derive(Debug, Clone)]
struct SessionEntry {
    session: Session,
    host_player_id: PlayerId,
    #[allow(dead_code)]
    password_hash: Option<String>,
    last_activity: Instant,
    connection_tokens: HashMap<String, Instant>,
}

/// Player connection state.
#[derive(Debug, Clone)]
struct PlayerConnection {
    #[allow(dead_code)]
    player_id: PlayerId,
    #[allow(dead_code)]
    sessions_subscribed: Vec<SessionId>,
    #[allow(dead_code)]
    last_heartbeat: Instant,
}

/// The lobby server.
pub struct LobbyServer {
    config: LobbyServerConfig,
    secret_key: Vec<u8>,
    sessions: Arc<RwLock<HashMap<u64, SessionEntry>>>,
    connections: Arc<RwLock<HashMap<SocketAddr, PlayerConnection>>>,
    next_session_id: Arc<RwLock<u64>>,
    event_tx: broadcast::Sender<ServerEvent>,
}

/// Events broadcast by the server.
#[derive(Debug, Clone, Serialize)]
pub enum ServerEvent {
    SessionCreated {
        session_id: SessionId,
    },
    SessionDestroyed {
        session_id: SessionId,
    },
    PlayerJoined {
        session_id: SessionId,
        player_id: PlayerId,
    },
    PlayerLeft {
        session_id: SessionId,
        player_id: PlayerId,
    },
    SessionStateChanged {
        session_id: SessionId,
        state: SessionState,
    },
}

impl LobbyServer {
    pub fn new(config: LobbyServerConfig) -> Self {
        let secret_key = config.resolve_secret();
        let (event_tx, _) = broadcast::channel(1024);
        Self {
            config,
            secret_key,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            connections: Arc::new(RwLock::new(HashMap::new())),
            next_session_id: Arc::new(RwLock::new(1)),
            event_tx,
        }
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<ServerEvent> {
        self.event_tx.subscribe()
    }

    pub async fn run(&self) -> std::io::Result<()> {
        let addr = format!("{}:{}", self.config.bind_address, self.config.port);
        let listener = TcpListener::bind(&addr).await?;
        log::info!("Lobby server listening on {}", addr);

        let cleanup_sessions = self.sessions.clone();
        let cleanup_interval = self.config.session_timeout_secs;
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                let mut sessions = cleanup_sessions.write().await;
                let now = Instant::now();
                let timeout = Duration::from_secs(cleanup_interval);
                sessions.retain(|_, entry| now.duration_since(entry.last_activity) < timeout);
            }
        });

        loop {
            let (stream, addr) = listener.accept().await?;
            let sessions = self.sessions.clone();
            let connections = self.connections.clone();
            let next_id = self.next_session_id.clone();
            let config = self.config.clone();
            let secret_key = self.secret_key.clone();
            let event_tx = self.event_tx.clone();

            tokio::spawn(async move {
                if let Err(e) = handle_connection(
                    stream,
                    addr,
                    sessions,
                    connections,
                    next_id,
                    config,
                    secret_key,
                    event_tx,
                )
                .await
                {
                    log::warn!("Connection error from {}: {}", addr, e);
                }
            });
        }
    }

    pub async fn session_count(&self) -> usize {
        self.sessions.read().await.len()
    }

    pub async fn player_count(&self) -> usize {
        self.connections.read().await.len()
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    addr: SocketAddr,
    sessions: Arc<RwLock<HashMap<u64, SessionEntry>>>,
    connections: Arc<RwLock<HashMap<SocketAddr, PlayerConnection>>>,
    next_session_id: Arc<RwLock<u64>>,
    config: LobbyServerConfig,
    secret_key: Vec<u8>,
    event_tx: broadcast::Sender<ServerEvent>,
) -> Result<(), LobbyError> {
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);

    connections.write().await.insert(
        addr,
        PlayerConnection {
            player_id: PlayerId::new(),
            sessions_subscribed: Vec::new(),
            last_heartbeat: Instant::now(),
        },
    );

    let mut request_buf = String::new();
    let timeout_duration = Duration::from_secs(30);

    loop {
        request_buf.clear();

        let result = timeout(timeout_duration, reader.read_line(&mut request_buf)).await;
        match result {
            Ok(Ok(0)) => break,
            Ok(Ok(_)) => {}
            Ok(Err(e)) => return Err(LobbyError::Network(e.to_string())),
            Err(_) => break,
        }

        if request_buf.starts_with("GET")
            || request_buf.starts_with("POST")
            || request_buf.starts_with("PATCH")
            || request_buf.starts_with("DELETE")
        {
            let response = handle_http_request(
                &mut reader,
                &request_buf,
                &sessions,
                &next_session_id,
                &config,
                &secret_key,
                &event_tx,
            )
            .await?;

            writer
                .write_all(response.as_bytes())
                .await
                .map_err(|e| LobbyError::Network(e.to_string()))?;
            writer
                .flush()
                .await
                .map_err(|e| LobbyError::Network(e.to_string()))?;
        }
    }

    connections.write().await.remove(&addr);
    Ok(())
}

async fn handle_http_request(
    reader: &mut BufReader<tokio::net::tcp::ReadHalf<'_>>,
    request_line: &str,
    sessions: &Arc<RwLock<HashMap<u64, SessionEntry>>>,
    next_session_id: &Arc<RwLock<u64>>,
    config: &LobbyServerConfig,
    secret_key: &[u8],
    event_tx: &broadcast::Sender<ServerEvent>,
) -> Result<String, LobbyError> {
    let mut headers = HashMap::new();
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader
            .read_line(&mut line)
            .await
            .map_err(|e| LobbyError::Network(e.to_string()))?;
        if bytes_read == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            headers.insert(key.trim().to_lowercase(), value.trim().to_string());
        }
    }

    let content_length: usize = headers
        .get("content-length")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    // CRIT-003: Reject requests exceeding maximum size limit
    if content_length > MAX_REQUEST_SIZE {
        return Ok(http_413_payload_too_large());
    }

    let mut body = Vec::new();
    if content_length > 0 {
        body.resize(content_length, 0);
        reader
            .read_exact(&mut body)
            .await
            .map_err(|e| LobbyError::Network(e.to_string()))?;
    }

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    let method = parts.first().unwrap_or(&"");
    let path = parts.get(1).unwrap_or(&"/");

    let (status, response_body) = route_request(
        method,
        path,
        &body,
        sessions,
        next_session_id,
        config,
        secret_key,
        event_tx,
    )
    .await;

    Ok(format!(
        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        status,
        response_body.len(),
        response_body
    ))
}

#[allow(clippy::too_many_arguments)]
async fn route_request(
    method: &str,
    path: &str,
    body: &[u8],
    sessions: &Arc<RwLock<HashMap<u64, SessionEntry>>>,
    next_session_id: &Arc<RwLock<u64>>,
    config: &LobbyServerConfig,
    secret_key: &[u8],
    event_tx: &broadcast::Sender<ServerEvent>,
) -> (u16, String) {
    // CRIT-003: Double-check body size at route level (defense in depth)
    if body.len() > MAX_REQUEST_SIZE {
        return (413, json_error("Payload too large"));
    }

    let body_str = String::from_utf8_lossy(body);

    if method == "POST" && path == "/api/sessions" {
        return create_session(&body_str, sessions, next_session_id, config, secret_key, event_tx).await;
    }

    if method == "GET" && path.starts_with("/api/sessions/") {
        let id_str = path.trim_start_matches("/api/sessions/");
        if let Ok(id) = u64::from_str_radix(id_str, 16) {
            return get_session(id, sessions).await;
        }
        return (404, json_error("Session not found"));
    }

    if method == "GET" && path == "/api/sessions" {
        return list_sessions(sessions).await;
    }

    if method == "POST" && path.ends_with("/join") {
        let id_str = path
            .trim_start_matches("/api/sessions/")
            .trim_end_matches("/join");
        if let Ok(id) = u64::from_str_radix(id_str, 16) {
            return join_session(id, &body_str, sessions, config, secret_key, event_tx).await;
        }
        return (404, json_error("Session not found"));
    }

    if method == "POST" && path.ends_with("/leave") {
        let id_str = path
            .trim_start_matches("/api/sessions/")
            .trim_end_matches("/leave");
        if let Ok(id) = u64::from_str_radix(id_str, 16) {
            return leave_session(id, &body_str, sessions, event_tx).await;
        }
        return (404, json_error("Session not found"));
    }

    if method == "DELETE" && path.starts_with("/api/sessions/") {
        let id_str = path.trim_start_matches("/api/sessions/");
        if let Ok(id) = u64::from_str_radix(id_str, 16) {
            return destroy_session(id, sessions, event_tx).await;
        }
        return (404, json_error("Session not found"));
    }

    if method == "PATCH" && path.ends_with("/state") {
        let id_str = path
            .trim_start_matches("/api/sessions/")
            .trim_end_matches("/state");
        if let Ok(id) = u64::from_str_radix(id_str, 16) {
            return update_session_state(id, &body_str, sessions, event_tx).await;
        }
        return (404, json_error("Session not found"));
    }

    if method == "PATCH" && path.ends_with("/player") {
        let id_str = path
            .trim_start_matches("/api/sessions/")
            .trim_end_matches("/player");
        if let Ok(id) = u64::from_str_radix(id_str, 16) {
            return update_player(id, &body_str, sessions).await;
        }
        return (404, json_error("Session not found"));
    }

    (404, json_error("Not found"))
}

async fn create_session(
    body: &str,
    sessions: &Arc<RwLock<HashMap<u64, SessionEntry>>>,
    next_session_id: &Arc<RwLock<u64>>,
    config: &LobbyServerConfig,
    secret_key: &[u8],
    event_tx: &broadcast::Sender<ServerEvent>,
) -> (u16, String) {
    // CRIT-003: Use safe deserialization with size and depth validation
    let request = match safe_deserialize::<CreateSessionRequest>(body) {
        Ok(req) => req,
        Err(SafeDeserializeError::TooLarge(_)) => {
            return (413, json_error("Payload too large"));
        }
        Err(SafeDeserializeError::StringTooLong(_)) => {
            return (400, json_error("String field exceeds maximum length"));
        }
        Err(SafeDeserializeError::Parse(_)) => {
            return (400, json_error("Invalid request body"));
        }
        Err(e) => {
            log::warn!("Unexpected deserialization error in create_session: {}", e);
            return (400, json_error("Invalid request body"));
        }
    };

    let mut sessions_lock = sessions.write().await;
    if sessions_lock.len() >= config.max_sessions {
        return (503, json_error("Server is full"));
    }

    let mut id_lock = next_session_id.write().await;
    let id = *id_lock;
    *id_lock += 1;

    let session_id = SessionId(id);
    let player_id = request.player_id.clone();

    let session = Session {
        id: session_id,
        config: request.config,
        player_count: 1,
        players: vec![PlayerInfo {
            id: player_id.clone(),
            name: format!("Player_{}", id),
            team: None,
            is_ready: false,
            metadata: HashMap::new(),
        }],
        state: SessionState::Lobby,
        server_address: None,
        created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };

    let connection_token = generate_token(session_id, &player_id, secret_key);

    sessions_lock.insert(
        id,
        SessionEntry {
            session: session.clone(),
            host_player_id: player_id.clone(),
            password_hash: None,
            last_activity: Instant::now(),
            connection_tokens: [(connection_token.clone(), Instant::now())]
                .into_iter()
                .collect(),
        },
    );

    drop(sessions_lock);

    let _ = event_tx.send(ServerEvent::SessionCreated { session_id });

    let response = CreateSessionResponse {
        session,
        connection_token,
        server_address: config.bind_address.clone(),
    };

    (200, serde_json::to_string(&response).unwrap_or_default())
}

async fn get_session(id: u64, sessions: &Arc<RwLock<HashMap<u64, SessionEntry>>>) -> (u16, String) {
    let sessions_lock = sessions.read().await;

    if let Some(entry) = sessions_lock.get(&id) {
        (
            200,
            serde_json::to_string(&entry.session).unwrap_or_default(),
        )
    } else {
        (404, json_error("Session not found"))
    }
}

async fn list_sessions(sessions: &Arc<RwLock<HashMap<u64, SessionEntry>>>) -> (u16, String) {
    let sessions_lock = sessions.read().await;
    let list: Vec<Session> = sessions_lock
        .values()
        .filter(|e| e.session.config.is_public && e.session.state == SessionState::Lobby)
        .map(|e| e.session.clone())
        .collect();

    (200, serde_json::to_string(&list).unwrap_or_default())
}

async fn join_session(
    id: u64,
    body: &str,
    sessions: &Arc<RwLock<HashMap<u64, SessionEntry>>>,
    config: &LobbyServerConfig,
    secret_key: &[u8],
    event_tx: &broadcast::Sender<ServerEvent>,
) -> (u16, String) {
    // CRIT-003: Use safe deserialization with size and depth validation
    let request = match safe_deserialize::<JoinSessionRequest>(body) {
        Ok(req) => req,
        Err(SafeDeserializeError::TooLarge(_)) => {
            return (413, json_error("Payload too large"));
        }
        Err(SafeDeserializeError::StringTooLong(_)) => {
            return (400, json_error("String field exceeds maximum length"));
        }
        Err(SafeDeserializeError::Parse(_)) => {
            return (400, json_error("Invalid request body"));
        }
        Err(e) => {
            log::warn!("Unexpected deserialization error in join_session: {}", e);
            return (400, json_error("Invalid request body"));
        }
    };

    let mut sessions_lock = sessions.write().await;

    let Some(entry) = sessions_lock.get_mut(&id) else {
        return (404, json_error("Session not found"));
    };

    if entry.session.players.len() as u32 >= entry.session.config.max_players {
        return (403, json_error("Session is full"));
    }

    let player_id = request.player_id.clone();
    let session_id = SessionId(id);

    entry.session.players.push(PlayerInfo {
        id: player_id.clone(),
        name: format!("Player_{}", player_id.0.chars().take(4).collect::<String>()),
        team: None,
        is_ready: false,
        metadata: HashMap::new(),
    });
    entry.session.player_count = entry.session.players.len() as u32;
    entry.last_activity = Instant::now();

    let connection_token = generate_token(session_id, &player_id, secret_key);
    entry
        .connection_tokens
        .insert(connection_token.clone(), Instant::now());

    let session = entry.session.clone();
    drop(sessions_lock);

    let _ = event_tx.send(ServerEvent::PlayerJoined {
        session_id,
        player_id,
    });

    let join_info = JoinInfo {
        session,
        connection_token,
        server_address: config.bind_address.clone(),
        player_id: request.player_id,
    };

    (
        200,
        serde_json::to_string(&JoinSessionResponse { join_info }).unwrap_or_default(),
    )
}

async fn leave_session(
    id: u64,
    body: &str,
    sessions: &Arc<RwLock<HashMap<u64, SessionEntry>>>,
    event_tx: &broadcast::Sender<ServerEvent>,
) -> (u16, String) {
    // CRIT-003: Use safe deserialization with size and depth validation
    let request = match safe_deserialize::<LeaveSessionRequest>(body) {
        Ok(req) => req,
        Err(SafeDeserializeError::TooLarge(_)) => {
            return (413, json_error("Payload too large"));
        }
        Err(SafeDeserializeError::StringTooLong(_)) => {
            return (400, json_error("String field exceeds maximum length"));
        }
        Err(SafeDeserializeError::Parse(_)) => {
            return (400, json_error("Invalid request body"));
        }
        Err(e) => {
            log::warn!("Unexpected deserialization error in leave_session: {}", e);
            return (400, json_error("Invalid request body"));
        }
    };

    let mut sessions_lock = sessions.write().await;

    let Some(entry) = sessions_lock.get_mut(&id) else {
        return (404, json_error("Session not found"));
    };

    let player_id = request.player_id.clone();
    let session_id = SessionId(id);

    entry.session.players.retain(|p| p.id != player_id);
    entry.session.player_count = entry.session.players.len() as u32;
    entry.last_activity = Instant::now();

    let was_host = entry.host_player_id == player_id;

    if was_host {
        if let Some(new_host) = entry.session.players.first() {
            entry.host_player_id = new_host.id.clone();
        }
    }

    drop(sessions_lock);

    let _ = event_tx.send(ServerEvent::PlayerLeft {
        session_id,
        player_id,
    });

    (200, "{}".to_string())
}

async fn destroy_session(
    id: u64,
    sessions: &Arc<RwLock<HashMap<u64, SessionEntry>>>,
    event_tx: &broadcast::Sender<ServerEvent>,
) -> (u16, String) {
    let mut sessions_lock = sessions.write().await;

    if sessions_lock.remove(&id).is_some() {
        drop(sessions_lock);
        let _ = event_tx.send(ServerEvent::SessionDestroyed {
            session_id: SessionId(id),
        });
        (200, "{}".to_string())
    } else {
        (404, json_error("Session not found"))
    }
}

async fn update_session_state(
    id: u64,
    body: &str,
    sessions: &Arc<RwLock<HashMap<u64, SessionEntry>>>,
    event_tx: &broadcast::Sender<ServerEvent>,
) -> (u16, String) {
    // CRIT-003: Use safe deserialization with size and depth validation
    let request = match safe_deserialize::<UpdateSessionRequest>(body) {
        Ok(req) => req,
        Err(SafeDeserializeError::TooLarge(_)) => {
            return (413, json_error("Payload too large"));
        }
        Err(SafeDeserializeError::StringTooLong(_)) => {
            return (400, json_error("String field exceeds maximum length"));
        }
        Err(SafeDeserializeError::Parse(_)) => {
            return (400, json_error("Invalid request body"));
        }
        Err(e) => {
            log::warn!("Unexpected deserialization error in update_session_state: {}", e);
            return (400, json_error("Invalid request body"));
        }
    };

    let mut sessions_lock = sessions.write().await;

    let Some(entry) = sessions_lock.get_mut(&id) else {
        return (404, json_error("Session not found"));
    };

    if entry.host_player_id != request.player_id {
        return (403, json_error("Only the host can change session state"));
    }

    entry.session.state = request.state;
    entry.last_activity = Instant::now();

    let state = entry.session.state;
    let session_id = SessionId(id);
    let session = entry.session.clone();

    drop(sessions_lock);

    let _ = event_tx.send(ServerEvent::SessionStateChanged { session_id, state });

    (200, serde_json::to_string(&session).unwrap_or_default())
}

async fn update_player(
    id: u64,
    body: &str,
    sessions: &Arc<RwLock<HashMap<u64, SessionEntry>>>,
) -> (u16, String) {
    // CRIT-003: Use safe deserialization with size and depth validation
    let request = match safe_deserialize::<UpdatePlayerRequest>(body) {
        Ok(req) => req,
        Err(SafeDeserializeError::TooLarge(_)) => {
            return (413, json_error("Payload too large"));
        }
        Err(SafeDeserializeError::StringTooLong(_)) => {
            return (400, json_error("String field exceeds maximum length"));
        }
        Err(SafeDeserializeError::Parse(_)) => {
            return (400, json_error("Invalid request body"));
        }
        Err(e) => {
            log::warn!("Unexpected deserialization error in update_player: {}", e);
            return (400, json_error("Invalid request body"));
        }
    };

    let mut sessions_lock = sessions.write().await;

    let Some(entry) = sessions_lock.get_mut(&id) else {
        return (404, json_error("Session not found"));
    };

    if let Some(player) = entry
        .session
        .players
        .iter_mut()
        .find(|p| p.id == request.player_id)
    {
        player.is_ready = request.is_ready;
        player.team = request.team;
        player.metadata = request.metadata;
        entry.last_activity = Instant::now();
        let session = entry.session.clone();
        drop(sessions_lock);
        (200, serde_json::to_string(&session).unwrap_or_default())
    } else {
        (404, json_error("Player not in session"))
    }
}

fn generate_token(session_id: SessionId, player_id: &PlayerId, secret: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&session_id.0.to_le_bytes());
    hasher.update(player_id.0.as_bytes());
    hasher.update(secret);
    hasher.update(
        &SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_le_bytes(),
    );

    let hash = hasher.finalize();
    format!("tok_{}_{}", session_id, hash.to_hex())
}

fn json_error(message: &str) -> String {
    serde_json::json!({ "error": message }).to_string()
}

// ─── Schema Validation Helpers (CRIT-003) ────────────────────────────────────

/// Deserializes a string field with length validation.
/// Returns an error if the string exceeds `MAX_STRING_LENGTH`.
fn deserialize_limited_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    if s.len() > MAX_STRING_LENGTH {
        return Err(serde::de::Error::custom(format!(
            "String length {} exceeds maximum {}",
            s.len(),
            MAX_STRING_LENGTH
        )));
    }
    Ok(s)
}

/// Deserializes a `PlayerId` with length validation on the underlying string.
fn deserialize_limited_player_id<'de, D>(deserializer: D) -> Result<PlayerId, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    if s.len() > MAX_STRING_LENGTH {
        return Err(serde::de::Error::custom(format!(
            "PlayerId length {} exceeds maximum {}",
            s.len(),
            MAX_STRING_LENGTH
        )));
    }
    Ok(PlayerId(s))
}

/// Deserializes an optional string field with length validation.
fn deserialize_optional_limited_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        Some(s) if s.len() > MAX_STRING_LENGTH => Err(serde::de::Error::custom(format!(
            "String length {} exceeds maximum {}",
            s.len(),
            MAX_STRING_LENGTH
        ))),
        Some(s) => Ok(Some(s)),
        None => Ok(None),
    }
}

/// Deserializes a HashMap<String, String> with length validation on both keys and values.
fn deserialize_limited_string_map<'de, D>(deserializer: D) -> Result<HashMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let map = HashMap::<String, String>::deserialize(deserializer)?;
    for (k, v) in &map {
        if k.len() > MAX_STRING_LENGTH {
            return Err(serde::de::Error::custom(format!(
                "Map key length {} exceeds maximum {}",
                k.len(),
                MAX_STRING_LENGTH
            )));
        }
        if v.len() > MAX_STRING_LENGTH {
            return Err(serde::de::Error::custom(format!(
                "Map value length {} exceeds maximum {}",
                v.len(),
                MAX_STRING_LENGTH
            )));
        }
    }
    Ok(map)
}

/// Request to create a new lobby session.
/// CRIT-003: All string fields are validated for length during deserialization.
#[derive(Debug, Serialize, Deserialize)]
struct CreateSessionRequest {
    config: SessionConfig,
    #[serde(deserialize_with = "deserialize_limited_player_id")]
    player_id: PlayerId,
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateSessionResponse {
    session: Session,
    #[serde(deserialize_with = "deserialize_limited_string")]
    connection_token: String,
    #[serde(deserialize_with = "deserialize_limited_string")]
    server_address: String,
}

/// Request to join an existing lobby session.
/// CRIT-003: All string fields are validated for length during deserialization.
#[derive(Debug, Serialize, Deserialize)]
struct JoinSessionRequest {
    session_id: SessionId,
    #[serde(deserialize_with = "deserialize_limited_player_id")]
    player_id: PlayerId,
    #[serde(default, deserialize_with = "deserialize_optional_limited_string")]
    password: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JoinSessionResponse {
    join_info: JoinInfo,
}

/// Request to leave a lobby session.
/// CRIT-003: All string fields are validated for length during deserialization.
#[derive(Debug, Serialize, Deserialize)]
struct LeaveSessionRequest {
    session_id: SessionId,
    #[serde(deserialize_with = "deserialize_limited_player_id")]
    player_id: PlayerId,
}

/// Request to update session state.
/// CRIT-003: All string fields are validated for length during deserialization.
#[derive(Debug, Serialize, Deserialize)]
struct UpdateSessionRequest {
    session_id: SessionId,
    #[serde(deserialize_with = "deserialize_limited_player_id")]
    player_id: PlayerId,
    state: SessionState,
}

/// Request to update player information.
/// CRIT-003: All string fields are validated for length during deserialization.
#[derive(Debug, Serialize, Deserialize)]
struct UpdatePlayerRequest {
    session_id: SessionId,
    #[serde(deserialize_with = "deserialize_limited_player_id")]
    player_id: PlayerId,
    is_ready: bool,
    team: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_limited_string_map")]
    metadata: HashMap<String, String>,
}

pub async fn run_server(config: LobbyServerConfig) -> std::io::Result<()> {
    let server = LobbyServer::new(config);
    server.run().await
}
