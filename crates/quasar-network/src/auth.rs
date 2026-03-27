//! Network authentication and security system.
//!
//! Provides:
//! - Token-based authentication
//! - Session management
//! - Rate limiting per client
//! - Ban list management

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Unique identifier for an authenticated client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub u64);

impl SessionId {
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Authentication credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub auth_token: String,
    pub client_version: String,
    pub timestamp: u64,
}

/// Authentication token issued by the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthToken {
    pub session_id: SessionId,
    pub player_id: u64,
    pub expires_at: u64,
    pub permissions: Vec<String>,
    pub signature: Vec<u8>,
}

/// Authentication result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthResult {
    Success {
        token: AuthToken,
    },
    Failed {
        reason: String,
    },
    Banned {
        reason: String,
        expires_at: Option<u64>,
    },
    RateLimited {
        retry_after_ms: u64,
    },
}

/// An active authenticated session.
#[derive(Debug, Clone)]
pub struct Session {
    pub id: SessionId,
    pub player_id: u64,
    pub username: String,
    pub addr: SocketAddr,
    pub permissions: HashSet<String>,
    pub created_at: Instant,
    pub last_activity: Instant,
    pub expires_at: Instant,
}

impl Session {
    pub fn is_expired(&self) -> bool {
        Instant::now() > self.expires_at
    }
    pub fn is_valid(&self) -> bool {
        !self.is_expired()
    }
    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.contains(permission)
    }
}

/// Session manager handles authenticated sessions.
#[derive(Debug)]
pub struct SessionManager {
    sessions: HashMap<SessionId, Session>,
    player_sessions: HashMap<u64, SessionId>,
    addr_sessions: HashMap<SocketAddr, SessionId>,
    session_timeout: Duration,
    max_sessions_per_ip: usize,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            player_sessions: HashMap::new(),
            addr_sessions: HashMap::new(),
            session_timeout: Duration::from_secs(3600),
            max_sessions_per_ip: 3,
        }
    }

    pub fn create_session(
        &mut self,
        player_id: u64,
        username: String,
        addr: SocketAddr,
        permissions: Vec<String>,
    ) -> Result<SessionId, String> {
        let ip_sessions = self
            .sessions
            .values()
            .filter(|s| s.addr.ip() == addr.ip())
            .count();
        if ip_sessions >= self.max_sessions_per_ip {
            return Err("Too many sessions from this IP".to_string());
        }

        if let Some(old_id) = self.player_sessions.get(&player_id) {
            self.remove_session(*old_id);
        }

        let session_id = SessionId::new();
        let now = Instant::now();
        let session = Session {
            id: session_id,
            player_id,
            username,
            addr,
            permissions: permissions.into_iter().collect(),
            created_at: now,
            last_activity: now,
            expires_at: now + self.session_timeout,
        };

        self.sessions.insert(session_id, session);
        self.player_sessions.insert(player_id, session_id);
        self.addr_sessions.insert(addr, session_id);
        Ok(session_id)
    }

    pub fn get_session(&self, id: SessionId) -> Option<&Session> {
        self.sessions.get(&id)
    }
    pub fn get_session_mut(&mut self, id: SessionId) -> Option<&mut Session> {
        self.sessions.get_mut(&id)
    }
    pub fn get_session_by_player(&self, player_id: u64) -> Option<&Session> {
        self.player_sessions
            .get(&player_id)
            .and_then(|id| self.sessions.get(id))
    }

    pub fn remove_session(&mut self, id: SessionId) {
        if let Some(session) = self.sessions.remove(&id) {
            self.player_sessions.remove(&session.player_id);
            self.addr_sessions.remove(&session.addr);
        }
    }

    pub fn cleanup_expired(&mut self) -> usize {
        let expired: Vec<_> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.is_expired())
            .map(|(id, _)| *id)
            .collect();
        let count = expired.len();
        for id in expired {
            self.remove_session(id);
        }
        count
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.session_timeout = timeout;
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// A ban entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BanEntry {
    pub player_id: Option<u64>,
    pub ip: Option<std::net::IpAddr>,
    pub reason: String,
    pub banned_at: u64,
    pub expires_at: Option<u64>,
    pub banned_by: Option<u64>,
}

impl BanEntry {
    pub fn is_expired(&self) -> bool {
        self.expires_at
            .map(|e| {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                now > e
            })
            .unwrap_or(false)
    }
}

/// Ban list manager.
#[derive(Debug, Clone, Default)]
pub struct BanList {
    player_bans: HashMap<u64, BanEntry>,
    ip_bans: HashMap<std::net::IpAddr, BanEntry>,
}

impl BanList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ban_player(
        &mut self,
        player_id: u64,
        reason: String,
        duration: Option<Duration>,
        banned_by: Option<u64>,
    ) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.player_bans.insert(
            player_id,
            BanEntry {
                player_id: Some(player_id),
                ip: None,
                reason,
                banned_at: now,
                expires_at: duration.map(|d| now + d.as_secs()),
                banned_by,
            },
        );
    }

    pub fn ban_ip(
        &mut self,
        ip: std::net::IpAddr,
        reason: String,
        duration: Option<Duration>,
        banned_by: Option<u64>,
    ) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.ip_bans.insert(
            ip,
            BanEntry {
                player_id: None,
                ip: Some(ip),
                reason,
                banned_at: now,
                expires_at: duration.map(|d| now + d.as_secs()),
                banned_by,
            },
        );
    }

    pub fn is_player_banned(&self, player_id: u64) -> Option<&BanEntry> {
        self.player_bans.get(&player_id).filter(|b| !b.is_expired())
    }

    pub fn is_ip_banned(&self, ip: &std::net::IpAddr) -> Option<&BanEntry> {
        self.ip_bans.get(ip).filter(|b| !b.is_expired())
    }
}

/// Rate limiter configuration.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub max_requests: usize,
    pub window_duration: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 10,
            window_duration: Duration::from_secs(60),
        }
    }
}

/// Global rate limiter.
#[derive(Debug)]
pub struct RateLimiter {
    clients: HashMap<SocketAddr, (Vec<Instant>, RateLimitConfig)>,
    config: RateLimitConfig,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            clients: HashMap::new(),
            config,
        }
    }

    pub fn check(&mut self, addr: &SocketAddr) -> Result<(), Duration> {
        let config = self.config.clone();
        let (requests, cfg) = self.clients.entry(*addr).or_insert((Vec::new(), config));
        let now = Instant::now();
        let cutoff = now - cfg.window_duration;
        requests.retain(|&t| t > cutoff);

        if requests.len() >= cfg.max_requests {
            let oldest = requests.first().copied().unwrap_or(now);
            return Err(oldest + cfg.window_duration - now);
        }
        requests.push(now);
        Ok(())
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(RateLimitConfig::default())
    }
}

/// Authentication server.
#[derive(Debug)]
pub struct AuthServer {
    #[allow(dead_code)]
    config: RateLimitConfig,
    sessions: SessionManager,
    bans: BanList,
    rate_limiter: RateLimiter,
    next_player_id: u64,
    secret_key: Vec<u8>,
}

impl AuthServer {
    pub fn new() -> Self {
        Self {
            config: RateLimitConfig::default(),
            sessions: SessionManager::new(),
            bans: BanList::new(),
            rate_limiter: RateLimiter::new(RateLimitConfig::default()),
            next_player_id: 1,
            secret_key: vec![0; 32],
        }
    }

    pub fn authenticate(&mut self, credentials: Credentials, addr: SocketAddr) -> AuthResult {
        if let Err(retry_after) = self.rate_limiter.check(&addr) {
            return AuthResult::RateLimited {
                retry_after_ms: retry_after.as_millis() as u64,
            };
        }

        if let Some(ban) = self.bans.is_ip_banned(&addr.ip()) {
            return AuthResult::Banned {
                reason: ban.reason.clone(),
                expires_at: ban.expires_at,
            };
        }

        if credentials.username.is_empty() || credentials.username.len() > 32 {
            return AuthResult::Failed {
                reason: "Invalid username".to_string(),
            };
        }

        let player_id = self.next_player_id;
        self.next_player_id += 1;

        match self.sessions.create_session(
            player_id,
            credentials.username.clone(),
            addr,
            vec!["play".into(), "chat".into()],
        ) {
            Ok(session_id) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                AuthResult::Success {
                    token: AuthToken {
                        session_id,
                        player_id,
                        expires_at: now + 3600,
                        permissions: vec!["play".into(), "chat".into()],
                        signature: self.sign_token(session_id, player_id, now + 3600),
                    },
                }
            }
            Err(reason) => AuthResult::Failed { reason },
        }
    }

    fn sign_token(&self, session_id: SessionId, player_id: u64, expires_at: u64) -> Vec<u8> {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        session_id.0.hash(&mut hasher);
        player_id.hash(&mut hasher);
        expires_at.hash(&mut hasher);
        self.secret_key.hash(&mut hasher);
        hasher.finish().to_be_bytes().to_vec()
    }

    pub fn validate_token(&self, token: &AuthToken) -> Result<&Session, String> {
        let expected = self.sign_token(token.session_id, token.player_id, token.expires_at);
        if token.signature != expected {
            return Err("Invalid signature".into());
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if token.expires_at < now {
            return Err("Token expired".into());
        }
        self.sessions
            .get_session(token.session_id)
            .filter(|s| s.is_valid())
            .ok_or("Session not found".into())
    }

    pub fn disconnect(&mut self, id: SessionId) {
        self.sessions.remove_session(id);
    }
    pub fn ban_player(&mut self, player_id: u64, reason: String, duration: Option<Duration>) {
        self.bans.ban_player(player_id, reason, duration, None);
        if let Some(s) = self.sessions.get_session_by_player(player_id) {
            self.sessions.remove_session(s.id);
        }
    }
    pub fn session_count(&self) -> usize {
        self.sessions.session_count()
    }
}

impl Default for AuthServer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_unique() {
        let a = SessionId::new();
        let b = SessionId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn session_manager_create() {
        let mut mgr = SessionManager::new();
        let addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let id = mgr
            .create_session(1, "player".into(), addr, vec!["play".into()])
            .unwrap();
        assert!(mgr.get_session(id).is_some());
        assert_eq!(mgr.session_count(), 1);
    }

    #[test]
    fn ban_list_ban_player() {
        let mut bans = BanList::new();
        bans.ban_player(123, "test".into(), None, None);
        assert!(bans.is_player_banned(123).is_some());
    }

    #[test]
    fn rate_limiter_limits() {
        let mut limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 2,
            window_duration: Duration::from_secs(60),
        });
        let addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        assert!(limiter.check(&addr).is_ok());
        assert!(limiter.check(&addr).is_ok());
        assert!(limiter.check(&addr).is_err());
    }

    #[test]
    fn auth_server_authenticate() {
        let mut server = AuthServer::new();
        let addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap();

        let result = server.authenticate(
            Credentials {
                username: "test".into(),
                auth_token: "".into(),
                client_version: "1.0".into(),
                timestamp: now,
            },
            addr,
        );

        assert!(matches!(result, AuthResult::Success { .. }));
    }
}
