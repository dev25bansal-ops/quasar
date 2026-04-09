//! Secret key management for lobby authentication and token signing.
//!
//! # Environment Variable
//!
//! The lobby secret **must** be set via the `QUASAR_LOBBY_SECRET` environment variable.
//! This secret is used for:
//! - HMAC token signing for session authentication
//! - Connection token generation and validation
//!
//! ## Setting the secret
//!
//! ```bash
//! # Linux/macOS
//! export QUASAR_LOBBY_SECRET="your-secret-key-at-least-32-bytes-long"
//!
//! # Windows (PowerShell)
//! $env:QUASAR_LOBBY_SECRET="your-secret-key-at-least-32-bytes-long"
//!
//! # Windows (CMD)
//! set QUASAR_LOBBY_SECRET=your-secret-key-at-least-32-bytes-long
//! ```
//!
//! ## Generating a secure secret
//!
//! Use a cryptographically secure random generator:
//! ```bash
//! # Using openssl
//! openssl rand -base64 48
//!
//! # Using Python
//! python -c "import secrets; print(secrets.token_urlsafe(48))"
//! ```
//!
//! ## Development mode
//!
//! When `QUASAR_DEV=1` is set, a warning is logged if no secret is configured
//! instead of panicking. This is for convenience during local development only
//! and **must never** be used in production.

use std::env;

/// Minimum required length for the lobby secret in bytes.
pub const MIN_SECRET_LENGTH: usize = 32;

/// Environment variable name for the lobby secret.
pub const SECRET_ENV_VAR: &str = "QUASAR_LOBBY_SECRET";

/// Environment variable to enable development mode (allows running without a secret).
pub const DEV_MODE_ENV_VAR: &str = "QUASAR_DEV";

/// Load and validate the lobby secret from the environment.
///
/// # Behavior
///
/// - **Production** (default): Panics if `QUASAR_LOBBY_SECRET` is not set or
///   if the secret is shorter than [`MIN_SECRET_LENGTH`] bytes.
/// - **Development** (`QUASAR_DEV=1`): Logs a warning and returns `None` if
///   the secret is not set. Returns `Some(secret)` if set, regardless of length
///   (though a warning is still emitted for short secrets).
///
/// # Returns
///
/// - `Some(Vec<u8>)` — the validated secret bytes.
/// - `None` — only in development mode when no secret is configured.
///
/// # Panics
///
/// Panics in production mode when:
/// - The `QUASAR_LOBBY_SECRET` environment variable is not set.
/// - The secret is shorter than [`MIN_SECRET_LENGTH`] bytes.
pub fn load_lobby_secret() -> Option<Vec<u8>> {
    let is_dev_mode = env::var(DEV_MODE_ENV_VAR).as_deref() == Ok("1");

    match env::var(SECRET_ENV_VAR) {
        Ok(secret) => {
            let bytes = secret.into_bytes();
            if bytes.len() < MIN_SECRET_LENGTH {
                if is_dev_mode {
                    log::warn!(
                        "Lobby secret is only {} bytes (minimum {}). This is insecure.",
                        bytes.len(),
                        MIN_SECRET_LENGTH
                    );
                } else {
                    panic!(
                        "CRITICAL: QUASAR_LOBBY_SECRET must be at least {} bytes long, but is {} bytes. \
                         Generate a secure secret with: openssl rand -base64 48",
                        MIN_SECRET_LENGTH,
                        bytes.len()
                    );
                }
            }
            Some(bytes)
        }
        Err(env::VarError::NotPresent) => {
            if is_dev_mode {
                log::warn!(
                    "No QUASAR_LOBBY_SECRET set — running in insecure dev mode. \
                     Set {} for production.",
                    SECRET_ENV_VAR
                );
                None
            } else {
                panic!(
                    "CRITICAL: QUASAR_LOBBY_SECRET environment variable is not set.\n\
                     \n\
                     This secret is required for secure token signing and authentication.\n\
                     Generate one with:\n\
                         openssl rand -base64 48\n\
                     \n\
                     Then set it in your environment:\n\
                         export QUASAR_LOBBY_SECRET=\"<your-secret>\"  (Linux/macOS)\n\
                         $env:QUASAR_LOBBY_SECRET = \"<your-secret>\"  (PowerShell)\n\
                         set QUASAR_LOBBY_SECRET=<your-secret>         (CMD)"
                );
            }
        }
        Err(e) => {
            panic!("CRITICAL: Failed to read QUASAR_LOBBY_SECRET: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_secret_length_is_32() {
        assert_eq!(MIN_SECRET_LENGTH, 32);
    }

    #[test]
    fn secret_env_var_name_is_correct() {
        assert_eq!(SECRET_ENV_VAR, "QUASAR_LOBBY_SECRET");
    }

    #[test]
    fn dev_mode_env_var_name_is_correct() {
        assert_eq!(DEV_MODE_ENV_VAR, "QUASAR_DEV");
    }
}
