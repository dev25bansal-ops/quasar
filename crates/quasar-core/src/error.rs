//! Centralized error types for the Quasar Engine.

use std::fmt;

/// Top-level engine error.
#[derive(Debug)]
pub enum QuasarError {
    /// GPU / rendering subsystem failure.
    Render(String),
    /// Window creation or event-loop failure.
    Window(String),
    /// Scripting engine failure.
    Script(String),
    /// Asset loading failure.
    Asset(String),
    /// Physics subsystem failure.
    Physics(String),
    /// Audio subsystem failure.
    Audio(String),
    /// Networking failure.
    Network(String),
    /// ECS / world operation failure.
    Ecs(String),
    /// Serialization / deserialization failure.
    Serialization(String),
    /// I/O failure.
    Io(std::io::Error),
    /// Generic / catch-all.
    Other(String),
}

impl fmt::Display for QuasarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Render(msg) => write!(f, "Render error: {msg}"),
            Self::Window(msg) => write!(f, "Window error: {msg}"),
            Self::Script(msg) => write!(f, "Script error: {msg}"),
            Self::Asset(msg) => write!(f, "Asset error: {msg}"),
            Self::Physics(msg) => write!(f, "Physics error: {msg}"),
            Self::Audio(msg) => write!(f, "Audio error: {msg}"),
            Self::Network(msg) => write!(f, "Network error: {msg}"),
            Self::Ecs(msg) => write!(f, "ECS error: {msg}"),
            Self::Serialization(msg) => write!(f, "Serialization error: {msg}"),
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::Other(msg) => write!(f, "Error: {msg}"),
        }
    }
}

impl std::error::Error for QuasarError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for QuasarError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<serde_json::Error> for QuasarError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}

/// Convenience alias used throughout the engine.
pub type QuasarResult<T> = Result<T, QuasarError>;
