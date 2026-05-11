//! Lobby client implementation.

use crate::secret;
use crate::{
    protocol::*, JoinInfo, LobbyError, PlayerId, Session, SessionConfig, SessionFilters, SessionId,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Configuration for the lobby client.
#[derive(Debug, Clone)]
pub struct LobbyClientConfig {
    /// Base URL of the lobby server.
    pub server_url: String,
    /// Request timeout in seconds.
    pub timeout_seconds: u64,
    /// Maximum number of retry attempts.
    pub max_retries: u32,
    /// API key for authentication (optional).
    pub api_key: Option<String>,
}

impl Default for LobbyClientConfig {
    fn default() -> Self {
        Self {
            server_url: "http://localhost:8080".to_string(),
            timeout_seconds: 30,
            max_retries: 3,
            api_key: None,
        }
    }
}

/// Client for interacting with a lobby server.
///
/// Provides methods for session management:
/// - `create_session`: Create a new game session
/// - `find_sessions`: Search for available sessions
/// - `join_session`: Join an existing session
/// - `leave_session`: Leave a session
/// - `get_session`: Get details of a specific session
#[derive(Debug, Clone)]
pub struct LobbyClient {
    #[allow(dead_code)]
    config: LobbyClientConfig,
    http_client: Arc<HttpClient>,
}

impl LobbyClient {
    /// Create a new lobby client with the given server URL.
    pub fn new(server_url: &str) -> Self {
        Self::with_config(LobbyClientConfig {
            server_url: server_url.to_string(),
            ..Default::default()
        })
    }

    /// Create a new lobby client with full configuration.
    pub fn with_config(config: LobbyClientConfig) -> Self {
        Self {
            config: config.clone(),
            http_client: Arc::new(HttpClient::new(config)),
        }
    }

    /// Create a new game session.
    ///
    /// Returns the created session along with connection information.
    pub async fn create_session(
        &self,
        config: SessionConfig,
    ) -> Result<(Session, JoinInfo), LobbyError> {
        let player_id = PlayerId(uuid_v4());
        let request = CreateSessionRequest {
            config,
            player_id: player_id.clone(),
        };

        let response: CreateSessionResponse =
            self.http_client.post("/api/sessions", &request).await?;

        Ok((
            response.session.clone(),
            JoinInfo {
                session: response.session,
                connection_token: response.connection_token,
                server_address: response.server_address,
                player_id,
            },
        ))
    }

    /// Find sessions matching the given filters.
    pub async fn find_sessions(&self, filters: SessionFilters) -> Result<Vec<Session>, LobbyError> {
        let query = build_query_string(&filters);
        let path = if query.is_empty() {
            "/api/sessions".to_string()
        } else {
            format!("/api/sessions?{}", query)
        };

        self.http_client.get(&path).await
    }

    /// Join an existing session.
    ///
    /// Returns connection information for the game server.
    pub async fn join_session(
        &self,
        session_id: SessionId,
        player_id: PlayerId,
        password: Option<String>,
    ) -> Result<JoinInfo, LobbyError> {
        let request = JoinSessionRequest {
            session_id,
            player_id: player_id.clone(),
            password,
        };

        let response: JoinSessionResponse = self
            .http_client
            .post(&format!("/api/sessions/{}/join", session_id), &request)
            .await?;

        Ok(response.join_info)
    }

    /// Leave a session.
    pub async fn leave_session(
        &self,
        session_id: SessionId,
        player_id: PlayerId,
    ) -> Result<(), LobbyError> {
        let request = LeaveSessionRequest {
            session_id,
            player_id,
        };

        self.http_client
            .post(&format!("/api/sessions/{}/leave", session_id), &request)
            .await
    }

    /// Get details of a specific session.
    pub async fn get_session(&self, session_id: SessionId) -> Result<Session, LobbyError> {
        self.http_client
            .get(&format!("/api/sessions/{}", session_id))
            .await
    }

    /// Update player state in a session.
    pub async fn update_player(
        &self,
        session_id: SessionId,
        player_id: PlayerId,
        is_ready: bool,
        team: Option<u32>,
        metadata: HashMap<String, String>,
    ) -> Result<Session, LobbyError> {
        let request = UpdatePlayerRequest {
            session_id,
            player_id,
            is_ready,
            team,
            metadata,
        };

        self.http_client
            .patch(&format!("/api/sessions/{}/player", session_id), &request)
            .await
    }

    /// Update session state (host only).
    pub async fn update_session_state(
        &self,
        session_id: SessionId,
        player_id: PlayerId,
        state: crate::SessionState,
    ) -> Result<Session, LobbyError> {
        let request = UpdateSessionRequest {
            session_id,
            player_id,
            state,
        };

        self.http_client
            .patch(&format!("/api/sessions/{}/state", session_id), &request)
            .await
    }

    /// Destroy a session (host only).
    pub async fn destroy_session(&self, session_id: SessionId) -> Result<(), LobbyError> {
        self.http_client
            .delete(&format!("/api/sessions/{}", session_id))
            .await
    }
}

/// Simple HTTP client for lobby API requests.
#[derive(Debug)]
struct HttpClient {
    base_url: String,
    timeout: Duration,
    api_key: Option<String>,
}

impl HttpClient {
    fn new(config: LobbyClientConfig) -> Self {
        Self {
            base_url: config.server_url,
            timeout: Duration::from_secs(config.timeout_seconds),
            api_key: config.api_key,
        }
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, LobbyError> {
        self.request("GET", path, None::<&()>).await
    }

    async fn post<T: serde::de::DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, LobbyError> {
        self.request("POST", path, Some(body)).await
    }

    async fn patch<T: serde::de::DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, LobbyError> {
        self.request("PATCH", path, Some(body)).await
    }

    async fn delete(&self, path: &str) -> Result<(), LobbyError> {
        self.request::<(), ()>("DELETE", path, None).await?;
        Ok(())
    }

    async fn request<T: serde::de::DeserializeOwned, B: Serialize>(
        &self,
        method: &str,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, LobbyError> {
        let url = format!("{}{}", self.base_url, path);

        // Parse URL to extract host and port
        let parsed = url_parser::parse_url(&url)?;
        let host = parsed.host;
        let port = parsed.port.unwrap_or(80);

        // Connect to server
        let addr = format!("{}:{}", host, port);
        let stream = timeout(self.timeout, TcpStream::connect(&addr))
            .await
            .map_err(|_| LobbyError::Network("Connection timeout".to_string()))?
            .map_err(|e| LobbyError::Network(format!("Failed to connect: {}", e)))?;

        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        // Build HTTP request
        let mut request = format!("{} {} HTTP/1.1\r\n", method, path);
        request.push_str(&format!("Host: {}\r\n", host));
        request.push_str("Content-Type: application/json\r\n");
        request.push_str("Accept: application/json\r\n");

        if let Some(ref api_key) = self.api_key {
            request.push_str(&format!("Authorization: Bearer {}\r\n", api_key));
        }

        let _body_str = if let Some(b) = body {
            let json =
                serde_json::to_string(b).map_err(|e| LobbyError::Serialization(e.to_string()))?;
            request.push_str(&format!("Content-Length: {}\r\n", json.len()));
            request.push_str("\r\n");
            request.push_str(&json);
            json
        } else {
            request.push_str("\r\n");
            String::new()
        };

        // Send request
        writer
            .write_all(request.as_bytes())
            .await
            .map_err(|e| LobbyError::Network(format!("Failed to send request: {}", e)))?;
        writer
            .flush()
            .await
            .map_err(|e| LobbyError::Network(format!("Failed to flush: {}", e)))?;

        // Read response
        let mut response_lines = Vec::new();
        let mut line = String::new();

        loop {
            line.clear();
            match timeout(self.timeout, reader.read_line(&mut line)).await {
                Ok(Ok(0)) => break,
                Ok(Ok(_)) => {
                    response_lines.push(line.clone());
                    if line == "\r\n" {
                        break;
                    }
                }
                Ok(Err(e)) => return Err(LobbyError::Network(format!("Failed to read: {}", e))),
                Err(_) => return Err(LobbyError::Network("Read timeout".to_string())),
            }
        }

        // Parse status line
        let status_line = response_lines
            .first()
            .ok_or_else(|| LobbyError::Network("Empty response".to_string()))?;
        let parts: Vec<&str> = status_line.split_whitespace().collect();
        if parts.len() < 2 {
            return Err(LobbyError::Network(format!(
                "Invalid status line: {}",
                status_line
            )));
        }

        let status_code: u16 = parts[1]
            .parse()
            .map_err(|_| LobbyError::Network(format!("Invalid status code: {}", parts[1])))?;

        if status_code >= 400 {
            return Err(LobbyError::Server {
                code: status_code,
                message: parts.get(2).unwrap_or(&"Unknown error").to_string(),
            });
        }

        // Read body
        let mut body_len = 0usize;
        for line in &response_lines {
            if line.to_lowercase().starts_with("content-length:") {
                body_len = line
                    .split(':')
                    .nth(1)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0);
                break;
            }
        }

        let mut body_bytes = vec![0u8; body_len];
        if body_len > 0 {
            timeout(self.timeout, reader.read_exact(&mut body_bytes))
                .await
                .map_err(|_| LobbyError::Network("Body read timeout".to_string()))?
                .map_err(|e| LobbyError::Network(format!("Failed to read body: {}", e)))?;
        }

        let body_str = String::from_utf8_lossy(&body_bytes);
        serde_json::from_str(&body_str)
            .map_err(|e| LobbyError::Serialization(format!("Failed to parse JSON: {}", e)))
    }
}

mod url_parser {
    use crate::LobbyError;

    pub struct ParsedUrl {
        pub host: String,
        pub port: Option<u16>,
    }

    pub fn parse_url(url: &str) -> Result<ParsedUrl, LobbyError> {
        let url = url.trim();

        // Remove protocol
        let without_protocol = if url.starts_with("http://") {
            url.trim_start_matches("http://")
        } else if url.starts_with("https://") {
            url.trim_start_matches("https://")
        } else {
            url
        };

        // Split on first '/' to get host:port
        let host_port = without_protocol
            .split('/')
            .next()
            .unwrap_or(without_protocol);

        // Split host and port
        if host_port.contains(':') {
            let parts: Vec<&str> = host_port.split(':').collect();
            let port: u16 = parts
                .get(1)
                .and_then(|p| p.parse().ok())
                .ok_or_else(|| LobbyError::Network(format!("Invalid URL: {}", url)))?;
            Ok(ParsedUrl {
                host: parts[0].to_string(),
                port: Some(port),
            })
        } else {
            Ok(ParsedUrl {
                host: host_port.to_string(),
                port: None,
            })
        }
    }
}

fn build_query_string(filters: &SessionFilters) -> String {
    let mut params = Vec::new();

    if let Some(ref mode) = filters.game_mode {
        params.push(format!("game_mode={}", url_encode(mode)));
    }
    if let Some(ref region) = filters.region {
        params.push(format!("region={}", url_encode(region)));
    }
    if let Some(min) = filters.min_players {
        params.push(format!("min_players={}", min));
    }
    if let Some(max) = filters.max_players {
        params.push(format!("max_players={}", max));
    }
    if let Some(ref state) = filters.state {
        params.push(format!("state={}", serde_json::to_string(state).unwrap()));
    }
    if let Some(limit) = filters.limit {
        params.push(format!("limit={}", limit));
    }

    for (key, value) in &filters.metadata {
        params.push(format!(
            "metadata[{}]={}",
            url_encode(key),
            url_encode(value)
        ));
    }

    params.join("&")
}

fn url_encode(s: &str) -> String {
    urlencoding::encode(s).to_string()
}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        s.chars()
            .map(|c| match c {
                'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
                _ => format!("%{:02X}", c as u8),
            })
            .collect()
    }
}

fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:016x}{:016x}", timestamp, rand_u64())
}

/// Generate a secure session token for authentication.
/// Uses a keyed BLAKE3 MAC with a secret key for token generation.
pub fn generate_session_token(
    session_id: SessionId,
    player_id: &PlayerId,
    secret: &[u8],
) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let issued_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let signature = session_token_signature(session_id, player_id, issued_at, secret);
    format!(
        "sess_{}_{}_{}_{}",
        session_id,
        hex_encode(player_id.0.as_bytes()),
        issued_at,
        signature.to_hex()
    )
}

/// Validate a session token.
/// Returns Ok((session_id, player_id)) if valid, Err otherwise.
pub fn validate_session_token(
    token: &str,
    secret: &[u8],
    max_age_secs: u64,
) -> Result<(SessionId, PlayerId), LobbyError> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let parts: Vec<&str> = token.splitn(5, '_').collect();
    if parts.len() != 5 || parts[0] != "sess" {
        return Err(LobbyError::InvalidPassword);
    }

    let session_id: SessionId = parts[1]
        .parse()
        .map_err(|_| LobbyError::SessionNotFound(SessionId(0)))?;
    let player_bytes = hex_decode(parts[2])?;
    let player_id =
        PlayerId(String::from_utf8(player_bytes).map_err(|_| LobbyError::InvalidPassword)?);
    let issued_at: u64 = parts[3].parse().map_err(|_| LobbyError::InvalidPassword)?;
    let provided = hex_decode(parts[4])?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if issued_at > now.saturating_add(60) {
        return Err(LobbyError::InvalidPassword);
    }
    if max_age_secs > 0 && now.saturating_sub(issued_at) > max_age_secs {
        return Err(LobbyError::InvalidPassword);
    }

    let expected = session_token_signature(session_id, &player_id, issued_at, secret);
    if !constant_time_eq(&provided, expected.as_bytes()) {
        return Err(LobbyError::InvalidPassword);
    }

    Ok((session_id, player_id))
}

fn session_token_signature(
    session_id: SessionId,
    player_id: &PlayerId,
    issued_at: u64,
    secret: &[u8],
) -> blake3::Hash {
    let key = blake3::hash(secret);
    let mut hasher = blake3::Hasher::new_keyed(key.as_bytes());
    hasher.update(b"quasar-lobby-session-v1");
    hasher.update(&session_id.0.to_le_bytes());
    hasher.update(&(player_id.0.len() as u64).to_le_bytes());
    hasher.update(player_id.0.as_bytes());
    hasher.update(&issued_at.to_le_bytes());
    hasher.finalize()
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn hex_decode(input: &str) -> Result<Vec<u8>, LobbyError> {
    if input.len() % 2 != 0 {
        return Err(LobbyError::InvalidPassword);
    }

    let mut out = Vec::with_capacity(input.len() / 2);
    for pair in input.as_bytes().chunks_exact(2) {
        let high = hex_value(pair[0])?;
        let low = hex_value(pair[1])?;
        out.push((high << 4) | low);
    }
    Ok(out)
}

fn hex_value(byte: u8) -> Result<u8, LobbyError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(LobbyError::InvalidPassword),
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut diff = 0u8;
    for (&left, &right) in a.iter().zip(b.iter()) {
        diff |= left ^ right;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"0123456789abcdef0123456789abcdef";

    #[test]
    fn session_token_round_trips_session_and_player() {
        let session_id = SessionId(42);
        let player_id = PlayerId("player-1".to_string());
        let token = generate_session_token(session_id, &player_id, SECRET);

        let validated = validate_session_token(&token, SECRET, 3600).expect("token should verify");

        assert_eq!(validated, (session_id, player_id));
    }

    #[test]
    fn session_token_rejects_wrong_secret() {
        let session_id = SessionId(42);
        let player_id = PlayerId("player-1".to_string());
        let token = generate_session_token(session_id, &player_id, SECRET);

        assert!(matches!(
            validate_session_token(&token, b"different secret with enough bytes", 3600),
            Err(LobbyError::InvalidPassword)
        ));
    }

    #[test]
    fn session_token_rejects_tampering() {
        let session_id = SessionId(42);
        let player_id = PlayerId("player-1".to_string());
        let token = generate_session_token(session_id, &player_id, SECRET);
        let tampered = token.replace(
            &hex_encode(player_id.0.as_bytes()),
            &hex_encode(b"player-2"),
        );

        assert!(matches!(
            validate_session_token(&tampered, SECRET, 3600),
            Err(LobbyError::InvalidPassword)
        ));
    }

    #[test]
    fn session_token_rejects_expired_tokens() {
        let session_id = SessionId(42);
        let player_id = PlayerId("player-1".to_string());
        let issued_at = 1;
        let signature = session_token_signature(session_id, &player_id, issued_at, SECRET);
        let token = format!(
            "sess_{}_{}_{}_{}",
            session_id,
            hex_encode(player_id.0.as_bytes()),
            issued_at,
            signature.to_hex()
        );

        assert!(matches!(
            validate_session_token(&token, SECRET, 1),
            Err(LobbyError::InvalidPassword)
        ));
    }
}

/// Auth configuration for lobby client.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// API key for server authentication.
    pub api_key: Option<String>,
    /// Secret key for token signing.
    pub secret: Option<Vec<u8>>,
    /// Token expiration time in seconds.
    pub token_expiry_secs: u64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            secret: None,
            token_expiry_secs: 3600, // 1 hour
        }
    }
}

impl LobbyClient {
    /// Create a new lobby client with authentication.
    pub fn with_auth(server_url: &str, auth: AuthConfig) -> Self {
        Self::with_config(LobbyClientConfig {
            server_url: server_url.to_string(),
            api_key: auth.api_key,
            ..Default::default()
        })
    }

    /// Generate an auth token for the given session and player.
    ///
    /// The secret is loaded from the `QUASAR_LOBBY_SECRET` environment variable.
    /// Returns `None` if no secret is configured (only possible in dev mode).
    pub fn create_auth_token(&self, session_id: SessionId, player_id: &PlayerId) -> Option<String> {
        let secret = secret::load_lobby_secret()?;
        Some(generate_session_token(session_id, player_id, &secret))
    }
}

fn rand_u64() -> u64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let state = RandomState::new();
    let mut hasher = state.build_hasher();
    hasher.write_u64(std::process::id() as u64);
    hasher.finish()
}

use serde::Serialize;
